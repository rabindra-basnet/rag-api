use axum::extract::{Multipart, Path as UrlPath, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiError;
use crate::middleware::auth::AuthUser;
use crate::state::app_state::AppState;

use crate::repository::file_repository;
use crate::handler::document_handler;
use crate::service::ocr_service;

const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Deserialize)]
pub struct UploadQuery {
    #[serde(default)]
    pub ingest: bool,
}

fn disk_path(state: &AppState, stored_name: &str) -> std::path::PathBuf {
    std::path::Path::new(&state.cfg.upload_dir).join(stored_name)
}

fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn is_extension_allowed(state: &AppState, filename: &str) -> bool {
    let allowed: Vec<String> = state
        .cfg
        .allowed_file_extensions
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if allowed.is_empty() {
        return true;
    }
    let name = filename.to_ascii_lowercase();
    allowed.iter().any(|ext| name.ends_with(ext.as_str()))
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

fn is_ingestable(content_type: &str, filename: &str) -> bool {
    is_texty(content_type, filename)
        || is_pdf(content_type, filename)
        || image_mime(content_type, filename).is_some()
}

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
        let models_dir = state.cfg.ocr_models_dir.clone();
        let bytes = data.to_vec();
        let text = tokio::task::spawn_blocking(move || ocr_service::extract_text(&models_dir, &bytes))
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

    let user_id = user.id.to_string();
    let mut saved: Vec<Value> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("invalid multipart body: {e}")))?
    {
        let Some(filename) = field.file_name().map(str::to_string) else {
            continue;
        };
        if filename.is_empty() || filename.len() > 255 {
            return Err(ApiError::BadRequest("bad filename".into()));
        }
        if !is_extension_allowed(&state, &filename) {
            return Err(ApiError::BadRequest(format!(
                "file type not allowed: {filename}"
            )));
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
        let stored_name = format!("{}_{}", Utc::now().timestamp(), sanitize_filename(&filename));
        tokio::fs::write(disk_path(&state, &stored_name), &data)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "file write failure");
                ApiError::Internal("failed to store file".into())
            })?;

        let mut document_id: Option<String> = None;
        if query.ingest {
            if let Some(text) = extract_text(&state, &content_type, &filename, &data).await? {
                let doc = document_handler::ingest_for_file(&state.cfg, &state.db, &user_id, &filename, &text)
                    .await
                    .inspect_err(|_| {
                        let p = disk_path(&state, &stored_name);
                        tokio::spawn(async move {
                            tokio::fs::remove_file(p).await.ok();
                        });
                    })?;
                document_id = Some(doc);
            }
        }

        file_repository::create(
            &state.db,
            &file_id,
            &user_id,
            &filename,
            &content_type,
            data.len() as i64,
            document_id.as_deref(),
            &Utc::now().to_rfc3339(),
            &stored_name,
        )
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

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Value>, ApiError> {
    let files = file_repository::list_by_user(&state.db, &user.id.to_string()).await?;
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

pub async fn ingest(
    State(state): State<AppState>,
    user: AuthUser,
    UrlPath(id): UrlPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let user_id = user.id.to_string();
    let file = file_repository::find_by_id(&state.db, &id.to_string(), &user_id)
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

    let stored = file.stored_name.clone().unwrap_or_else(|| file.id.clone());
    let data = tokio::fs::read(disk_path(&state, &stored)).await.map_err(|e| {
        tracing::error!(error = %e, file_id = file.id, "stored file missing");
        ApiError::NotFound
    })?;

    let text = extract_text(&state, &file.content_type, &file.filename, &data)
        .await?
        .ok_or_else(|| ApiError::BadRequest("no extractable text in file".into()))?;

    let document_id =
        document_handler::ingest_for_file(&state.cfg, &state.db, &user_id, &file.filename, &text).await?;

    file_repository::update_document_id(&state.db, &file.id, &document_id).await?;

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
    let file = file_repository::find_by_id(&state.db, &id.to_string(), &user.id.to_string())
        .await?
        .ok_or(ApiError::NotFound)?;

    let stored = file.stored_name.clone().unwrap_or_else(|| file.id.clone());
    let data = tokio::fs::read(disk_path(&state, &stored)).await.map_err(|e| {
        tracing::error!(error = %e, file_id = file.id, "stored file missing");
        ApiError::NotFound
    })?;

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
    let file = file_repository::find_by_id(&state.db, &id.to_string(), &user.id.to_string()).await?;
    let deleted = file_repository::delete(&state.db, &id.to_string(), &user.id.to_string()).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }
    let stored = file
        .map(|f| f.stored_name.unwrap_or_else(|| id.to_string()))
        .unwrap_or_else(|| id.to_string());
    tokio::fs::remove_file(disk_path(&state, &stored)).await.ok();
    Ok(Json(json!({ "ok": true })))
}
