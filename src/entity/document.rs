use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Document {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub user_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub embedding: Vec<u8>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DocumentSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub chunks: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct ChunkHit {
    pub content: String,
    pub document_id: String,
    pub title: String,
    pub embedding: Vec<u8>,
}
