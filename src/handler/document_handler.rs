use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::parameter::Config;
use crate::dto::document_dto::{IngestBody, IngestReq, TextIngestQuery};
use crate::error::ApiError;
use crate::middleware::auth::AuthUser;
use crate::state::app_state::AppState;
use crate::error::request_error::ValidatedJson;

use crate::service::chunker_service;
use crate::service::llm_service;
use crate::repository::document_repository;

const MAX_DOCUMENT_CHARS: usize = 10 * 1024 * 1024;
const EMBED_BATCH: usize = 64;

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

async fn ingest_one(
    cfg: &Config,
    db: &crate::config::database::Db,
    user_id: &str,
    title: &str,
    content: &str,
) -> Result<Value, ApiError> {
    let chunks = chunker_service::chunk_text(content, cfg.chunk_max_tokens, cfg.chunk_overlap_tokens);
    if chunks.is_empty() {
        return Err(ApiError::BadRequest("no usable content".into()));
    }

    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
    for batch in chunks.chunks(EMBED_BATCH) {
        embeddings.extend(llm_service::embed(cfg, batch).await?);
    }

    let doc_id = Uuid::new_v4();
    let mut tx = db.begin().await?;
    document_repository::insert_document(
        &mut tx,
        &doc_id.to_string(),
        user_id,
        title,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    document_repository::insert_chunks(&mut tx, &doc_id.to_string(), user_id, &chunks, &embeddings).await?;
    tx.commit().await?;

    Ok(json!({ "id": doc_id, "title": title, "chunks": chunks.len() }))
}

pub async fn ingest_for_file(
    cfg: &Config,
    db: &crate::config::database::Db,
    user_id: &str,
    filename: &str,
    text: &str,
) -> Result<String, ApiError> {
    let title = filename.trim();
    validate_doc(title, text)?;
    let result = ingest_one(cfg, db, user_id, title, text).await?;
    Ok(result["id"].as_str().unwrap_or_default().to_string())
}

pub async fn ingest_document(
    State(state): State<AppState>,
    user: AuthUser,
    axum::extract::Query(query): axum::extract::Query<TextIngestQuery>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let user_id = user.id.to_string();
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
            "unsupported content-type: {content_type}"
        )));
    };

    if docs.is_empty() {
        return Err(ApiError::BadRequest("no documents provided".into()));
    }
    if docs.len() > 50 {
        return Err(ApiError::BadRequest("too many documents (max 50)".into()));
    }
    for d in &docs {
        validate_doc(d.title.trim(), &d.content)?;
    }

    let mut results = Vec::with_capacity(docs.len());
    for d in &docs {
        results.push(ingest_one(&state.cfg, &state.db, &user_id, d.title.trim(), &d.content).await?);
    }

    if results.len() == 1 {
        Ok(Json(results.into_iter().next().unwrap()))
    } else {
        Ok(Json(json!({ "documents": results, "count": results.len() })))
    }
}

pub async fn update_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<IngestReq>,
) -> Result<Json<Value>, ApiError> {
    let user_id = user.id.to_string();
    let title = req.title.trim();
    validate_doc(title, &req.content)?;

    let owned = document_repository::find_document(&state.db, &id.to_string(), &user_id).await?;
    if owned.is_none() {
        return Err(ApiError::NotFound);
    }

    let chunks = chunker_service::chunk_text(&req.content, state.cfg.chunk_max_tokens, state.cfg.chunk_overlap_tokens);
    if chunks.is_empty() {
        return Err(ApiError::BadRequest("no usable content".into()));
    }
    let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
    for batch in chunks.chunks(EMBED_BATCH) {
        embeddings.extend(llm_service::embed(&state.cfg, batch).await?);
    }

    let mut tx = state.db.begin().await?;
    document_repository::update_document_title(&mut tx, &id.to_string(), title).await?;
    document_repository::delete_chunks(&mut tx, &id.to_string()).await?;
    document_repository::insert_chunks(&mut tx, &id.to_string(), &user_id, &chunks, &embeddings).await?;
    tx.commit().await?;

    Ok(Json(json!({ "id": id, "title": title, "chunks": chunks.len() })))
}

pub async fn list_documents(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let docs = document_repository::list_documents(&state.db, &user.id.to_string()).await?;
    Ok(Json(json!({ "documents": docs })))
}

pub async fn delete_document(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let deleted = document_repository::delete_document(&state.db, &id.to_string(), &user.id.to_string()).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}
