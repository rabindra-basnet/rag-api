//! File uploads from the frontend: multipart POST saved to UPLOAD_DIR,
//! metadata in the `files` table. Text files can be ingested straight
//! into the RAG index with `?ingest=true`.

use axum::extract::{Multipart, Path as UrlPath, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::error::ApiError;
use crate::models::FileRecord;
use crate::state::AppState;

const MAX_FILE_BYTES: usize = 10 * 1024 * 1024; // 10 MB each

#[derive(Deserialize)]
pub struct UploadQuery {
    /// When true and the file is text, also ingest it as a RAG document.
    #[serde(default)]
    pub ingest: bool,
}

/// Files are stored on disk as "<uuid>" only — the client filename never
/// touches the filesystem, so path traversal is impossible.
fn disk_path(state: &AppState, file_id: &str) -> std::path::PathBuf {
    std::path::Path::new(&state.cfg.upload_dir).join(file_id)
}

fn is_texty(content_type: &str, filename: &str) -> bool {
    content_type.starts_with("text/")
        || content_type == "application/json"
        || content_type == "application/xml"
        || [".txt", ".md", ".csv", ".json", ".xml", ".html"]
            .iter()
            .any(|ext| filename.to_lowercase().ends_with(ext))
}

fn is_pdf(content_type: &str, filename: &str) -> bool {
    content_type == "application/pdf" || filename.to_lowercase().ends_with(".pdf")
}

/// Image types the vision-LLM OCR path accepts.
fn image_mime(content_type: &str, filename: &str) -> Option<&'static str> {
    let name = filename.to_lowercase();
    match content_type {
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" => {
            Some(match content_type {
                "image/png" => "image/png",
                "image/jpeg" => "image/jpeg",
                "image/webp" => "image/webp",
                _ => "image/gif",
            })
        }
        _ if name.ends_with(".png") => Some("image/png"),
        _ if name.ends_with(".jpg") || name.ends_with(".jpeg") => Some("image/jpeg"),
        _ if name.ends_with(".webp") => Some("image/webp"),
        _ if name.ends_with(".gif") => Some("image/gif"),
        _ => None,
    }
}

/// Extract ingestable text from an uploaded file, by type: text-ish files
/// as utf-8, PDFs via pdf-extract, images via vision-LLM OCR (no system
/// OCR dependency). Returns Ok(None) for types we can't extract from.
async fn extract_text(
    state: &AppState,
    content_type: &str,
    filename: &str,
    data: &[u8],
) -> Result<Option<String>, ApiError> {
    if is_texty(content_type, filename) {
        return Ok(std::str::from_utf8(data).ok().map(str::to_string));
    }
    if is_pdf(content_type, filename) {
        // CPU-heavy parse — keep it off the async workers.
        let bytes = data.to_vec();
        let text = tokio::task::spawn_blocking(move || pdf_extract::extract_text_from_mem(&bytes))
            .await
            .map_err(|e| ApiError::Internal(format!("pdf task: {e}")))?
            .map_err(|e| {
                tracing::warn!(error = %e, "pdf text extraction failed");
                ApiError::BadRequest("could not extract text from pdf".into())
            })?;
        let text = text.trim().to_string();
        return Ok((!text.is_empty()).then_some(text));
    }
    if image_mime(content_type, filename).is_some() {
        // Local OCR (pure-Rust ocrs engine) — CPU-bound, keep off async workers.
        let models_dir = state.cfg.ocr_models_dir.clone();
        let bytes = data.to_vec();
        let text =
            tokio::task::spawn_blocking(move || crate::ocr::extract_text(&models_dir, &bytes))
                .await
                .map_err(|e| ApiError::Internal(format!("ocr task: {e}")))??;
        return Ok(Some(text));
    }
    Ok(None)
}

pub async fn upload(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    tokio::fs::create_dir_all(&state.cfg.upload_dir)
        .await
        .map_err(|e| ApiError::Internal(format!("upload dir: {e}")))?;

    let mut saved: Vec<Value> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("invalid multipart body: {e}")))?
    {
        // Only process file fields (skip plain form values).
        let Some(filename) = field.file_name().map(str::to_string) else {
            continue;
        };
        if filename.is_empty() || filename.len() > 255 {
            return Err(ApiError::BadRequest("bad filename".into()));
        }
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let data = field
            .bytes()
            .await
            .map_err(|e| ApiError::BadRequest(format!("failed reading upload: {e}")))?;
        if data.is_empty() {
            return Err(ApiError::BadRequest("empty file".into()));
        }
        if data.len() > MAX_FILE_BYTES {
            return Err(ApiError::BadRequest("file too large (max 10 MB)".into()));
        }

        let file_id = Uuid::new_v4().to_string();
        tokio::fs::write(disk_path(&state, &file_id), &data)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "file write failure");
                ApiError::Internal("failed to store file".into())
            })?;

        // Optionally extract text (plain/markdown/json/..., PDF) and ingest
        // it into the RAG index.
        let mut document_id: Option<String> = None;
        if query.ingest {
            if let Some(text) = extract_text(&state, &content_type, &filename, &data).await? {
                let doc = crate::rag::handlers::ingest_for_file(&state, &user, &filename, &text)
                    .await
                    .inspect_err(|_| {
                        // Don't leave the orphaned blob behind if ingestion fails.
                        let p = disk_path(&state, &file_id);
                        tokio::spawn(async move {
                            tokio::fs::remove_file(p).await.ok();
                        });
                    })?;
                document_id = Some(doc);
            }
        }

        sqlx::query(
            "INSERT INTO files (id, user_id, filename, content_type, size_bytes, document_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&file_id)
        .bind(user.id.to_string())
        .bind(&filename)
        .bind(&content_type)
        .bind(data.len() as i64)
        .bind(&document_id)
        .bind(Utc::now().to_rfc3339())
        .execute(&state.db)
        .await?;

        saved.push(json!({
            "id": file_id,
            "filename": filename,
            "content_type": content_type,
            "size_bytes": data.len(),
            "document_id": document_id,
        }));
    }

    if saved.is_empty() {
        return Err(ApiError::BadRequest("no file field in multipart body".into()));
    }
    Ok(Json(json!({ "files": saved, "count": saved.len() })))
}

/// True when we know how to extract text from this file type.
fn is_ingestable(content_type: &str, filename: &str) -> bool {
    is_texty(content_type, filename)
        || is_pdf(content_type, filename)
        || image_mime(content_type, filename).is_some()
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let files: Vec<FileRecord> =
        sqlx::query_as("SELECT * FROM files WHERE user_id = ? ORDER BY created_at DESC")
            .bind(user.id.to_string())
            .fetch_all(&state.db)
            .await?;

    let files: Vec<Value> = files
        .into_iter()
        .map(|f| {
            let ingestable = is_ingestable(&f.content_type, &f.filename);
            let ingested = f.document_id.is_some();
            let mut v = serde_json::to_value(&f).unwrap_or_default();
            v["ingestable"] = json!(ingestable);
            v["ingested"] = json!(ingested);
            v
        })
        .collect();
    Ok(Json(json!({ "files": files })))
}

/// Ingest an already-uploaded file into the RAG index.
pub async fn ingest(
    State(state): State<AppState>,
    user: AuthUser,
    UrlPath(id): UrlPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let file: FileRecord = sqlx::query_as("SELECT * FROM files WHERE id = ? AND user_id = ?")
        .bind(id.to_string())
        .bind(user.id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    if file.document_id.is_some() {
        return Err(ApiError::Conflict("file already ingested".into()));
    }
    if !is_ingestable(&file.content_type, &file.filename) {
        return Err(ApiError::BadRequest(format!(
            "cannot extract text from {} files",
            file.content_type
        )));
    }

    let data = tokio::fs::read(disk_path(&state, &file.id)).await.map_err(|e| {
        tracing::error!(error = %e, file_id = file.id, "stored file missing");
        ApiError::NotFound
    })?;

    let text = extract_text(&state, &file.content_type, &file.filename, &data)
        .await?
        .ok_or_else(|| ApiError::BadRequest("no extractable text in file".into()))?;

    let document_id =
        crate::rag::handlers::ingest_for_file(&state, &user, &file.filename, &text).await?;

    sqlx::query("UPDATE files SET document_id = ? WHERE id = ?")
        .bind(&document_id)
        .bind(&file.id)
        .execute(&state.db)
        .await?;

    Ok(Json(json!({
        "id": file.id,
        "document_id": document_id,
        "ingested": true,
    })))
}

pub async fn download(
    State(state): State<AppState>,
    user: AuthUser,
    UrlPath(id): UrlPath<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let file: FileRecord = sqlx::query_as("SELECT * FROM files WHERE id = ? AND user_id = ?")
        .bind(id.to_string())
        .bind(user.id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    let data = tokio::fs::read(disk_path(&state, &file.id)).await.map_err(|e| {
        tracing::error!(error = %e, file_id = file.id, "stored file missing");
        ApiError::NotFound
    })?;

    // Quote-safe ASCII fallback name; real name preserved in the JSON list.
    let safe_name: String = file
        .filename
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();

    Ok((
        [
            (header::CONTENT_TYPE, file.content_type),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{safe_name}\""),
            ),
        ],
        data,
    ))
}

pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    UrlPath(id): UrlPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM files WHERE id = ? AND user_id = ?")
        .bind(id.to_string())
        .bind(user.id.to_string())
        .execute(&state.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    tokio::fs::remove_file(disk_path(&state, &id.to_string()))
        .await
        .ok(); // DB row is the source of truth; a missing blob is not an error
    Ok(Json(json!({ "ok": true })))
}
