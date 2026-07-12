use crate::db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub cfg: Config,
    /// Shared HTTP client for the OpenAI-compatible upstreams.
    pub http: reqwest::Client,
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(110)) // under the 120s route timeout
        .build()
        .expect("reqwest client")
}

#[derive(Clone)]
pub struct Config {
    /// "development" or "production".
    pub environment: String,
    pub jwt_secret: String,
    /// Separate signing key for refresh JWTs: a leaked/cracked access-token
    /// key can never forge refresh tokens, and vice versa.
    pub refresh_jwt_secret: String,
    pub access_ttl_minutes: i64,
    pub refresh_ttl_days: i64,
    pub cookie_secure: bool,
    pub trust_proxy: bool,
    pub database_url: String,
    pub bind_addr: String,
    pub upload_dir: String,
    // Chat completions (OpenRouter or any OpenAI-compatible API)
    pub llm_base_url: String,
    pub llm_api_key: Option<String>,
    pub llm_model: String,
    pub llm_max_tokens: u32,
    /// Directory holding the ocrs .rten model files for local image OCR.
    pub ocr_models_dir: String,
    // Embeddings (OpenRouter doesn't serve embeddings, so this can point elsewhere,
    // e.g. OpenAI, Together, Jina, or a local server like Ollama/llama.cpp)
    pub embeddings_base_url: String,
    pub embeddings_api_key: Option<String>,
    pub embeddings_model: String,
}

/// Env var with a fallback default.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// First set env var out of several aliases, if any.
fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| std::env::var(k).ok())
}

/// Env var parsed to any FromStr type, falling back to `default` when
/// missing or unparsable.
fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Boolean env var: "true"/"1" -> true, anything else -> false, unset -> default.
fn env_flag(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v == "true" || v == "1")
        .unwrap_or(default)
}

impl Config {
    pub fn from_env() -> Self {
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!("JWT_SECRET not set — generating an ephemeral secret (tokens won't survive restarts)");
            use rand::RngCore;
            let mut buf = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut buf);
            hex::encode(buf)
        });
        // Prefer an independent secret; otherwise derive one from JWT_SECRET
        // so the two key spaces are still distinct.
        let refresh_jwt_secret = std::env::var("REFRESH_JWT_SECRET").unwrap_or_else(|_| {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(
                format!("{jwt_secret}:refresh-v1").as_bytes(),
            ))
        });
        let llm_base_url = env_or("LLM_BASE_URL", "https://openrouter.ai/api/v1");
        let llm_api_key = env_first(&["LLM_API_KEY", "OPENROUTER_API_KEY"]);
        Self {
            environment: env_or("ENVIRONMENT", "development"),
            jwt_secret,
            refresh_jwt_secret,
            access_ttl_minutes: env_parse("ACCESS_TTL_MINUTES", 15),
            refresh_ttl_days: env_parse("REFRESH_TTL_DAYS", 30),
            // Secure by default; set COOKIE_SECURE=false for plain-http dev.
            cookie_secure: env_flag("COOKIE_SECURE", true),
            // Only trust Forwarded/X-Forwarded-For for rate-limit keying when
            // actually behind a proxy that overwrites them; otherwise a
            // direct client could spoof the header to dodge the limit.
            trust_proxy: env_flag("TRUST_PROXY", false),
            database_url: env_or("DATABASE_URL", "sqlite://rag.db?mode=rwc"),
            bind_addr: env_or("BIND_ADDR", "0.0.0.0:3000"),
            upload_dir: env_or("UPLOAD_DIR", "./uploads"),
            llm_model: env_first(&["LLM_MODEL", "OPENROUTER_MODEL"])
                .unwrap_or_else(|| "meta-llama/llama-3.3-70b-instruct".into()),
            llm_max_tokens: env_parse("LLM_MAX_TOKENS", 1024),
            ocr_models_dir: env_or("OCR_MODELS_DIR", "./models"),
            // Default to the same provider as chat; override only if your
            // chat provider doesn't serve embeddings.
            embeddings_base_url: env_or("EMBEDDINGS_BASE_URL", &llm_base_url),
            embeddings_api_key: env_first(&["EMBEDDINGS_API_KEY"]).or_else(|| llm_api_key.clone()),
            embeddings_model: env_or("EMBEDDINGS_MODEL", "text-embedding-3-small"),
            llm_base_url,
            llm_api_key,
        }
    }
}
