-- Revoked access/refresh token IDs (jti) for server-side logout.
-- Short-lived: rows are pruned once the token's expiry passes.

CREATE TABLE IF NOT EXISTS revoked_tokens (
    jti TEXT PRIMARY KEY,
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expires ON revoked_tokens(expires_at);
