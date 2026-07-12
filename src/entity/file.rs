use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct FileRecord {
    pub id: String,
    pub user_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub document_id: Option<String>,
    pub created_at: String,
    pub stored_name: Option<String>,
}
