use crate::config::database::Db;
use crate::entity::file::FileRecord;
use crate::error::ApiError;

pub async fn find_by_id(db: &Db, id: &str, user_id: &str) -> Result<Option<FileRecord>, ApiError> {
    let file = sqlx::query_as::<_, FileRecord>("SELECT * FROM files WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(file)
}

pub async fn list_by_user(db: &Db, user_id: &str) -> Result<Vec<FileRecord>, ApiError> {
    let files = sqlx::query_as::<_, FileRecord>(
        "SELECT * FROM files WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(files)
}

pub async fn create(
    db: &Db,
    id: &str,
    user_id: &str,
    filename: &str,
    content_type: &str,
    size_bytes: i64,
    document_id: Option<&str>,
    created_at: &str,
    stored_name: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO files (id, user_id, filename, content_type, size_bytes, document_id, created_at, stored_name)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(user_id)
    .bind(filename)
    .bind(content_type)
    .bind(size_bytes)
    .bind(document_id)
    .bind(created_at)
    .bind(stored_name)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn update_document_id(
    db: &Db,
    file_id: &str,
    document_id: &str,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE files SET document_id = ? WHERE id = ?")
        .bind(document_id)
        .bind(file_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn delete(db: &Db, id: &str, user_id: &str) -> Result<bool, ApiError> {
    let result = sqlx::query("DELETE FROM files WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}
