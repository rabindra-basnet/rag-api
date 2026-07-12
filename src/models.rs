//! Database models — one struct per table (plus query-shaped projections),
//! mapped with sqlx::FromRow. Raw SQL stays in the repositories/handlers;
//! these give the rows names and types.

use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct RefreshToken {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub family_id: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub created_at: String,
}

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
pub struct FileRecord {
    pub id: String,
    pub user_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub document_id: Option<String>,
    pub created_at: String,
}

/// Projection for GET /documents (join with chunk count).
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DocumentSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub chunks: i64,
}

/// Projection for retrieval: chunk joined with its document title.
#[derive(Debug, Clone, FromRow)]
pub struct ChunkHit {
    pub content: String,
    pub document_id: String,
    pub title: String,
    pub embedding: Vec<u8>,
}
