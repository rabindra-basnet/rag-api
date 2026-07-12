use crate::config::database::{Db, DbTx};
use crate::entity::document::{ChunkHit, Document, DocumentSummary};
use crate::error::ApiError;
use uuid::Uuid;

pub async fn find_document(db: &Db, id: &str, user_id: &str) -> Result<Option<Document>, ApiError> {
    let doc = sqlx::query_as::<_, Document>("SELECT * FROM documents WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(doc)
}

pub async fn list_documents(db: &Db, user_id: &str) -> Result<Vec<DocumentSummary>, ApiError> {
    let docs = sqlx::query_as::<_, DocumentSummary>(
        "SELECT d.id, d.title, d.created_at, COUNT(c.id) AS chunks
         FROM documents d LEFT JOIN chunks c ON c.document_id = d.id
         WHERE d.user_id = ?
         GROUP BY d.id ORDER BY d.created_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(docs)
}

pub async fn insert_document(
    tx: &mut DbTx<'_>,
    doc_id: &str,
    user_id: &str,
    title: &str,
    created_at: &str,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO documents (id, user_id, title, created_at) VALUES (?, ?, ?, ?)")
        .bind(doc_id)
        .bind(user_id)
        .bind(title)
        .bind(created_at)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn update_document_title(
    tx: &mut DbTx<'_>,
    doc_id: &str,
    title: &str,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE documents SET title = ? WHERE id = ?")
        .bind(title)
        .bind(doc_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn delete_document(db: &Db, id: &str, user_id: &str) -> Result<bool, ApiError> {
    let result = sqlx::query("DELETE FROM documents WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn insert_chunks(
    tx: &mut DbTx<'_>,
    doc_id: &str,
    user_id: &str,
    chunks: &[String],
    embeddings: &[Vec<f32>],
) -> Result<(), ApiError> {
    for (i, (content, emb)) in chunks.iter().zip(embeddings).enumerate() {
        sqlx::query(
            "INSERT INTO chunks (id, document_id, user_id, chunk_index, content, embedding)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(doc_id)
        .bind(user_id)
        .bind(i as i64)
        .bind(content)
        .bind(crate::service::vector_service::encode(emb))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn delete_chunks(tx: &mut DbTx<'_>, doc_id: &str) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM chunks WHERE document_id = ?")
        .bind(doc_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn all_chunks_for_user(db: &Db, user_id: &str) -> Result<Vec<ChunkHit>, ApiError> {
    let rows = sqlx::query_as::<_, ChunkHit>(
        "SELECT c.content, c.document_id, d.title, c.embedding
         FROM chunks c JOIN documents d ON d.id = c.document_id
         WHERE c.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}
