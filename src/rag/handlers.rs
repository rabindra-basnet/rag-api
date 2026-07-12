use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::error::ApiError;
use crate::llm::{self, ChatMessage};
use crate::state::AppState;

use super::chunker::chunk_text;
use super::vectors;

const CHUNK_MAX_CHARS: usize = 1500;
const CHUNK_OVERLAP_CHARS: usize = 200;
const MAX_DOCUMENT_CHARS: usize = 500_000;
const TOP_K: usize = 5;
const EMBED_BATCH: usize = 64;

#[derive(Deserialize)]
pub struct IngestReq {
    pub title: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct ChatReq {
    pub question: String,
    #[serde(default)]
    pub top_k: Option<usize>,
}

pub async fn ingest_document(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<IngestReq>,
) -> Result<Json<Value>, ApiError> {
    let title = req.title.trim();
    if title.is_empty() || title.len() > 500 {
        return Err(ApiError::BadRequest("title must be 1-500 characters".into()));
    }
    if req.content.trim().is_empty() {
        return Err(ApiError::BadRequest("content is empty".into()));
    }
    if req.content.len() > MAX_DOCUMENT_CHARS {
        return Err(ApiError::BadRequest("document too large".into()));
    }

    let chunks = chunk_text(&req.content, CHUNK_MAX_CHARS, CHUNK_OVERLAP_CHARS);
    if chunks.is_empty() {
        return Err(ApiError::BadRequest("no usable content".into()));
    }

    // Embed before writing anything, so a failed upstream call leaves no
    // half-ingested document behind.
    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
    for batch in chunks.chunks(EMBED_BATCH) {
        embeddings.extend(llm::embed(&state, batch).await?);
    }

    let doc_id = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO documents (id, user_id, title, created_at) VALUES (?, ?, ?, ?)")
        .bind(doc_id.to_string())
        .bind(user.id.to_string())
        .bind(title)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;

    for (i, (content, emb)) in chunks.iter().zip(&embeddings).enumerate() {
        sqlx::query(
            "INSERT INTO chunks (id, document_id, user_id, chunk_index, content, embedding)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(doc_id.to_string())
        .bind(user.id.to_string())
        .bind(i as i64)
        .bind(content)
        .bind(vectors::encode(emb))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(Json(json!({
        "id": doc_id,
        "title": title,
        "chunks": chunks.len(),
    })))
}

pub async fn list_documents(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT d.id, d.title, d.created_at, COUNT(c.id)
         FROM documents d LEFT JOIN chunks c ON c.document_id = d.id
         WHERE d.user_id = ?
         GROUP BY d.id ORDER BY d.created_at DESC",
    )
    .bind(user.id.to_string())
    .fetch_all(&state.db)
    .await?;

    let docs: Vec<Value> = rows
        .into_iter()
        .map(|(id, title, created_at, chunks)| {
            json!({ "id": id, "title": title, "created_at": created_at, "chunks": chunks })
        })
        .collect();
    Ok(Json(json!({ "documents": docs })))
}

pub async fn delete_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM documents WHERE id = ? AND user_id = ?")
        .bind(id.to_string())
        .bind(user.id.to_string())
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn chat(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ChatReq>,
) -> Result<Json<Value>, ApiError> {
    let question = req.question.trim();
    if question.is_empty() || question.len() > 4000 {
        return Err(ApiError::BadRequest("question must be 1-4000 characters".into()));
    }
    let top_k = req.top_k.unwrap_or(TOP_K).clamp(1, 20);

    let query_emb = llm::embed(&state, std::slice::from_ref(&question.to_string()))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::Internal("no query embedding".into()))?;

    // Brute-force cosine scan over the user's chunks. Fine for SQLite scale;
    // swap for a vector index (e.g. sqlite-vec, Qdrant) when corpora grow.
    let rows: Vec<(String, String, String, Vec<u8>)> = sqlx::query_as(
        "SELECT c.content, c.document_id, d.title, c.embedding
         FROM chunks c JOIN documents d ON d.id = c.document_id
         WHERE c.user_id = ?",
    )
    .bind(user.id.to_string())
    .fetch_all(&state.db)
    .await?;

    let mut scored: Vec<(f32, String, String, String)> = rows
        .into_iter()
        .map(|(content, doc_id, title, blob)| {
            let sim = vectors::cosine_similarity(&query_emb, &vectors::decode(&blob));
            (sim, content, doc_id, title)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    if scored.is_empty() {
        return Ok(Json(json!({
            "answer": "No documents have been ingested yet. Upload documents first, then ask again.",
            "sources": [],
        })));
    }

    let mut context = String::new();
    for (i, (_, content, _, title)) in scored.iter().enumerate() {
        context.push_str(&format!("[{}] (from \"{}\")\n{}\n\n", i + 1, title, content));
    }

    let system = "You are a helpful assistant. Answer the user's question using ONLY the \
                  provided context passages. Cite passages by their [number]. If the context \
                  does not contain the answer, say so plainly instead of guessing."
        .to_string();
    let user_msg = format!("Context passages:\n\n{context}\nQuestion: {question}");

    let answer = llm::chat_completion(
        &state,
        vec![
            ChatMessage { role: "system", content: system },
            ChatMessage { role: "user", content: user_msg },
        ],
    )
    .await?;

    let sources: Vec<Value> = scored
        .iter()
        .enumerate()
        .map(|(i, (score, content, doc_id, title))| {
            json!({
                "index": i + 1,
                "document_id": doc_id,
                "title": title,
                "score": score,
                "snippet": content.chars().take(200).collect::<String>(),
            })
        })
        .collect();

    Ok(Json(json!({ "answer": answer, "sources": sources })))
}
