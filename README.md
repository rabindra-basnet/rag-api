# rag-backend

A Retrieval-Augmented Generation (RAG) backend in Rust. Users register, upload text documents, and ask questions answered by an LLM grounded in their own documents — with source citations.

Built with **axum**, **SQLite (sqlx)**, and **async-openai** against OpenRouter or any OpenAI-compatible API.

## How it works

```
ingest:  document ──▶ chunk (¶/sentence-aware, overlapping) ──▶ embed ──▶ SQLite (f32 blobs)
query:   question ──▶ embed ──▶ cosine top-k over user's chunks ──▶ LLM with context ──▶ answer + sources
```

Retrieval is a brute-force cosine scan over the user's chunks — simple and fine at SQLite scale. Swap in `sqlite-vec` or a vector DB when corpora grow.

## Quick start

```bash
cp .env.example .env   # then set JWT_SECRET and LLM_API_KEY
cargo run
```

The database file is created automatically on first run. Try the API with [`rest.http`](rest.http) (VS Code REST Client) — run **Register** or **Login** first and the auth token chains into the other requests.

## Configuration

All via environment variables / `.env`:

| Variable | Default | Notes |
|---|---|---|
| `BIND_ADDR` | `0.0.0.0:3000` | |
| `DATABASE_URL` | `sqlite://rag.db?mode=rwc` | `rwc` = read-write-create |
| `JWT_SECRET` | random (ephemeral) | **Set this** — otherwise tokens die on restart |
| `ACCESS_TTL_MINUTES` | `15` | Access token lifetime |
| `REFRESH_TTL_DAYS` | `30` | Refresh token lifetime |
| `LLM_BASE_URL` | `https://openrouter.ai/api/v1` | Any OpenAI-compatible API |
| `LLM_API_KEY` | — | Also reads `OPENROUTER_API_KEY` |
| `LLM_MODEL` | `meta-llama/llama-3.3-70b-instruct` | `google/gemma-4-26b-a4b-it:free` works free |
| `LLM_MAX_TOKENS` | `1024` | Cap for low-credit accounts |
| `EMBEDDINGS_BASE_URL` | same as `LLM_BASE_URL` | Override for a separate provider |
| `EMBEDDINGS_API_KEY` | same as `LLM_API_KEY` | |
| `EMBEDDINGS_MODEL` | `text-embedding-3-small` | `nvidia/llama-nemotron-embed-vl-1b-v2:free` works free on OpenRouter |

> Changing the embeddings model makes previously ingested vectors unretrievable (different dimensions/space) — re-ingest documents after switching.

## API

Auth endpoints are public; everything else needs `Authorization: Bearer <access_token>`.

| Method | Path | Body | Description |
|---|---|---|---|
| GET | `/health` | — | Liveness check |
| POST | `/auth/register` | `{email, password}` | Create account, returns tokens |
| POST | `/auth/login` | `{email, password}` | Returns access + refresh tokens |
| POST | `/auth/refresh` | `{refresh_token}` | Rotates refresh, new access token |
| POST | `/auth/logout` | `{refresh_token}` | Revokes the refresh token |
| GET | `/auth/me` | — | Current user |
| POST | `/documents` | see below | Ingest one or many documents |
| PUT | `/documents/{id}` | `{title, content}` | Replace content: re-chunks + re-embeds atomically |
| GET | `/documents` | — | List own documents |
| DELETE | `/documents/{id}` | — | Delete document + its chunks |
| POST | `/chat` | `{question, top_k?}` | RAG answer with cited sources |
| POST | `/files[?ingest=true]` | multipart | Upload files (10 MB each); optionally ingest text/PDF into RAG |
| GET | `/files` | — | List files with `ingestable` and `ingested` status |
| POST | `/files/{id}/ingest` | — | Ingest an already-uploaded file (409 if already ingested) |
| GET | `/files/{id}` | — | Download the original file |
| DELETE | `/files/{id}` | — | Delete file (blob + record) |

`POST /documents` accepts, by Content-Type (documents up to 10 MB each, max 50 per batch):

- `application/json`: `{title, content}` or an array `[{title, content}, ...]`
- `text/plain`: raw body is the content, title via `?title=` query param

Refresh tokens are returned in the JSON body **and** set as an `HttpOnly; SameSite=Strict` cookie scoped to `/auth` — browser clients can call `/auth/refresh` and `/auth/logout` with no body at all. Set `COOKIE_SECURE=true` behind HTTPS.

Example chat response:

```json
{
  "answer": "Rust was created by Graydon Hoare at Mozilla Research [1].",
  "sources": [
    { "index": 1, "document_id": "…", "title": "Rust Facts", "score": 0.65, "snippet": "…" }
  ]
}
```

## Production middleware (tower)

Request pipeline: panic recovery → `x-request-id` generation/propagation → auth-header redaction in traces → HTTP tracing → 120 s timeout (LLM calls are slow) → CORS → gzip compression → 50 MiB body cap. Graceful shutdown drains connections on SIGINT/SIGTERM. Request bodies are validated declaratively (`validator` crate) via a `ValidatedJson` extractor that returns structured JSON 400s.

## Rate limiting

Per-IP token buckets (`tower_governor`): `/auth/*` refills 1 req/s with a burst of 10 (brute-force protection); all authenticated routes refill 5 req/s with a burst of 50. Over-limit requests get `429` with `Retry-After`.

Behind nginx, set `TRUST_PROXY=true` so limits key on the real client IP, and make sure nginx sets the forwarding headers:

```nginx
location / {
    proxy_pass http://127.0.0.1:3000;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header Host $host;
}
```

With `TRUST_PROXY=false` (default) the peer address is used, so a direct client can't spoof `X-Forwarded-For` to dodge the limit.

## Security

- **Access tokens**: HS256 JWTs with pinned algorithm, `iss`/`aud` validation, required `exp`/`nbf`, `jti`, and a `typ: "access"` claim so refresh tokens can never pass as access tokens.
- **Refresh tokens**: JWTs typed `"refresh"`, but stateful — every token's SHA-256 hash lives in the DB, so a valid signature alone is never enough. Rotated on every use; replaying a rotated token revokes the entire token family (reuse detection). Also delivered as an `HttpOnly` cookie for browsers.
- **Passwords**: Argon2id hashes; login verifies against a dummy hash for missing users to keep response timing uniform (no user enumeration).
- All documents, chunks, and queries are scoped per user.

## Project layout

```
src/
├── main.rs            # router, middleware layers, startup
├── state.rs           # AppState, env config, OpenAI-compatible clients
├── db.rs              # SQLite pool + schema (users, refresh_tokens, documents, chunks)
├── error.rs           # ApiError → HTTP responses
├── llm.rs             # chat completions + embeddings via async-openai
├── auth/
│   ├── handlers.rs    # register/login/refresh/logout/me
│   ├── jwt.rs         # access token issue/verify
│   ├── tokens.rs      # refresh token rotation + family revocation
│   ├── middleware.rs  # Bearer auth extractor
│   └── password.rs    # Argon2 hash/verify
└── rag/
    ├── chunker.rs     # overlapping paragraph/sentence chunking
    ├── vectors.rs     # f32 blob encode/decode, cosine similarity
    └── handlers.rs    # ingest, list, delete, chat
```

## Tests

```bash
cargo test
```

Unit tests cover chunking and vector math. For a manual end-to-end run, use `rest.http` against a running server.
