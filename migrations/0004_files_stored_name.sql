-- Stored on-disk filename (timestamp-prefixed, sanitized original name).
ALTER TABLE files ADD COLUMN stored_name TEXT;
