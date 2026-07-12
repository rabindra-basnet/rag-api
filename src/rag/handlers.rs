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
const MAX_DOCUMENT_CHARS: usize = 10 * 1024 * 1024; // 10 MB, matches the body limit
const TOP_K: usize = 5;
const EMBED_BATCH: usize = 64;

#[derive(Deserialize, validator::Validate)]
pub struct IngestReq {
    #[validate(length(min = 1, max = 500, message = "title must be 1-500 characters"))]
    pub title: String,
    #[validate(length(min = 1, message = "content is empty"))]
    pub content: String,
}

/// JSON body may be a single document or an array of documents.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum IngestBody {
    One(IngestReq),
    Many(Vec<IngestReq>),
}

#[derive(Deserialize)]
pub struct TextIngestQuery {
    pub title: Option<String>,
}

#[derive(Deserialize, validator::Validate)]
pub struct ChatReq {
    #[validate(length(min = 1, max = 4000, message = "question must be 1-4000 characters"))]
    pub question: String,
    #[validate(range(min = 1, max = 20, message = "top_k must be 1-20"))]
    #[serde(default)]
    pub top_k: Option<usize>,
}

fn validate_doc(title: &str, content: &str) -> Result<(), ApiError> {
    if title.is_empty() || title.len() > 500 {
        return Err(ApiError::BadRequest("title must be 1-500 characters".into()));
    }
    if content.trim().is_empty() {
        return Err(ApiError::BadRequest("content is empty".into()));
    }
    if content.len() > MAX_DOCUMENT_CHARS {
        return Err(ApiError::BadRequest("document too large".into()));
    }
    Ok(())
}

/// Chunk + embed + store one document. Embeds before writing anything, so a
/// failed upstream call leaves no half-ingested document behind.
async fn ingest_one(
    state: &AppState,
    user: &AuthUser,
    title: &str,
    content: &str,
) -> Result<Value, ApiError> {
    let chunks = chunk_text(content, CHUNK_MAX_CHARS, CHUNK_OVERLAP_CHARS);
    if chunks.is_empty() {
        return Err(ApiError::BadRequest("no usable content".into()));
    }

    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
    for batch in chunks.chunks(EMBED_BATCH) {
        embeddings.extend(llm::embed(state, batch).await?);
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
    insert_chunks(&mut tx, doc_id, user.id, &chunks, &embeddings).await?;
    tx.commit().await?;

    Ok(json!({ "id": doc_id, "title": title, "chunks": chunks.len() }))
}

async fn insert_chunks(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    doc_id: Uuid,
    user_id: Uuid,
    chunks: &[String],
    embeddings: &[Vec<f32>],
) -> Result<(), ApiError> {
    for (i, (content, emb)) in chunks.iter().zip(embeddings).enumerate() {
        sqlx::query(
            "INSERT INTO chunks (id, document_id, user_id, chunk_index, content, embedding)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(doc_id.to_string())
        .bind(user_id.to_string())
        .bind(i as i64)
        .bind(content)
        .bind(vectors::encode(emb))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// POST /documents — Content-Type dependent:
/// - `application/json`: `{title, content}` or `[{title, content}, ...]`
/// - `text/plain`: raw body is the content; title from `?title=` (or "Untitled")
pub async fn ingest_document(
    State(state): State<AppState>,
    user: AuthUser,
    axum::extract::Query(query): axum::extract::Query<TextIngestQuery>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    let docs: Vec<IngestReq> = if content_type.starts_with("application/json") {
        match serde_json::from_slice::<IngestBody>(&body)
            .map_err(|e| ApiError::BadRequest(format!("invalid json body: {e}")))?
        {
            IngestBody::One(d) => vec![d],
            IngestBody::Many(ds) => ds,
        }
    } else if content_type.starts_with("text/plain") || content_type.is_empty() {
        let content = String::from_utf8(body.to_vec())
            .map_err(|_| ApiError::BadRequest("body is not valid utf-8".into()))?;
        vec![IngestReq {
            title: query.title.unwrap_or_else(|| "Untitled".into()),
            content,
        }]
    } else {
        return Err(ApiError::BadRequest(format!(
            "unsupported content-type: {content_type} (use application/json or text/plain)"
        )));
    };

    if docs.is_empty() {
        return Err(ApiError::BadRequest("no documents provided".into()));
    }
    if docs.len() > 50 {
        return Err(ApiError::BadRequest("too many documents (max 50)".into()));
    }
    // Validate everything up front so a bad document fails the batch early.
    for d in &docs {
        validate_doc(d.title.trim(), &d.content)?;
    }

    let mut results = Vec::with_capacity(docs.len());
    for d in &docs {
        results.push(ingest_one(&state, &user, d.title.trim(), &d.content).await?);
    }

    if results.len() == 1 {
        Ok(Json(results.into_iter().next().unwrap()))
    } else {
        Ok(Json(json!({ "documents": results, "count": results.len() })))
    }
}

/// PUT /documents/{id} — replace title/content: re-chunks, re-embeds, and
/// swaps all chunks atomically.
pub async fn update_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    crate::validation::ValidatedJson(req): crate::validation::ValidatedJson<IngestReq>,
) -> Result<Json<Value>, ApiError> {
    let title = req.title.trim();
    validate_doc(title, &req.content)?;

    let owned: Option<(String,)> =
        sqlx::query_as("SELECT id FROM documents WHERE id = ? AND user_id = ?")
            .bind(id.to_string())
            .bind(user.id.to_string())
            .fetch_optional(&state.db)
            .await?;
    if owned.is_none() {
        return Err(ApiError::NotFound);
    }

    let chunks = chunk_text(&req.content, CHUNK_MAX_CHARS, CHUNK_OVERLAP_CHARS);
    if chunks.is_empty() {
        return Err(ApiError::BadRequest("no usable content".into()));
    }
    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
    for batch in chunks.chunks(EMBED_BATCH) {
        embeddings.extend(llm::embed(&state, batch).await?);
    }

    // Old chunks stay live until the new ones are ready; swap is atomic.
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE documents SET title = ? WHERE id = ?")
        .bind(title)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM chunks WHERE document_id = ?")
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
    insert_chunks(&mut tx, id, user.id, &chunks, &embeddings).await?;
    tx.commit().await?;

    Ok(Json(json!({ "id": id, "title": title, "chunks": chunks.len() })))
}

pub async fn list_documents(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let docs: Vec<crate::models::DocumentSummary> = sqlx::query_as(
        "SELECT d.id, d.title, d.created_at, COUNT(c.id) AS chunks
         FROM documents d LEFT JOIN chunks c ON c.document_id = d.id
         WHERE d.user_id = ?
         GROUP BY d.id ORDER BY d.created_at DESC",
    )
    .bind(user.id.to_string())
    .fetch_all(&state.db)
    .await?;

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
    crate::validation::ValidatedJson(req): crate::validation::ValidatedJson<ChatReq>,
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
    let rows: Vec<crate::models::ChunkHit> = sqlx::query_as(
        "SELECT c.content, c.document_id, d.title, c.embedding
         FROM chunks c JOIN documents d ON d.id = c.document_id
         WHERE c.user_id = ?",
    )
    .bind(user.id.to_string())
    .fetch_all(&state.db)
    .await?;

    let mut scored: Vec<(f32, crate::models::ChunkHit)> = rows
        .into_iter()
        .map(|hit| {
            let sim = vectors::cosine_similarity(&query_emb, &vectors::decode(&hit.embedding));
            (sim, hit)
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
    for (i, (_, hit)) in scored.iter().enumerate() {
        context.push_str(&format!("[{}] (from \"{}\")\n{}\n\n", i + 1, hit.title, hit.content));
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
        .map(|(i, (score, hit))| {
            json!({
                "index": i + 1,
                "document_id": hit.document_id,
                "title": hit.title,
                "score": score,
                "snippet": hit.content.chars().take(200).collect::<String>(),
            })
        })
        .collect();

    Ok(Json(json!({ "answer": answer, "sources": sources })))
}
