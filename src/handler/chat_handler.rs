use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::dto::document_dto::ChatReq;
use crate::error::ApiError;
use crate::middleware::auth::AuthUser;
use crate::state::app_state::AppState;
use crate::error::request_error::ValidatedJson;

use crate::service::llm_service::{self, ChatMessage};
use crate::service::vector_service;
use crate::repository::document_repository;

const TOP_K: usize = 5;

pub async fn chat(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<ChatReq>,
) -> Result<Json<Value>, ApiError> {
    let question = req.question.trim();
    if question.is_empty() || question.len() > 4000 {
        return Err(ApiError::BadRequest("question must be 1-4000 characters".into()));
    }
    let top_k = req.top_k.unwrap_or(TOP_K).clamp(1, 20);

    let query_emb = llm_service::embed(&state.cfg, std::slice::from_ref(&question.to_string()))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::Internal("no query embedding".into()))?;

    let rows = document_repository::all_chunks_for_user(&state.db, &user.id.to_string()).await?;

    let mut scored: Vec<(f32, crate::entity::document::ChunkHit)> = rows
        .into_iter()
        .map(|hit| {
            let sim = vector_service::cosine_similarity(&query_emb, &vector_service::decode(&hit.embedding));
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

    let answer = llm_service::chat_completion(&state.cfg, vec![
        ChatMessage { role: "system", content: system },
        ChatMessage { role: "user", content: user_msg },
    ])
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
