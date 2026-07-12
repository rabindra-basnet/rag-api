CREATE TABLE files (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Original client filename (display only; never used as a disk path).
    filename     TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes   INTEGER NOT NULL,
    -- Set when the file's text content was ingested as a RAG document.
    document_id  TEXT REFERENCES documents(id) ON DELETE SET NULL,
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_files_user ON files(user_id);
