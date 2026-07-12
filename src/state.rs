use async_openai::config::OpenAIConfig;
use async_openai::Client;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub cfg: Config,
    /// Chat-completions client (OpenRouter or any OpenAI-compatible API).
    pub llm: Client<OpenAIConfig>,
    /// Embeddings client; may point at a different provider than `llm`.
    pub embeddings: Client<OpenAIConfig>,
}

pub fn openai_client(base_url: &str, api_key: &Option<String>) -> Client<OpenAIConfig> {
    let mut cfg = OpenAIConfig::new().with_api_base(base_url.trim_end_matches('/'));
    if let Some(key) = api_key {
        cfg = cfg.with_api_key(key.clone());
    }
    Client::with_config(cfg)
}

#[derive(Clone)]
pub struct Config {
    pub jwt_secret: String,
    pub access_ttl_minutes: i64,
    pub refresh_ttl_days: i64,
    pub database_url: String,
    pub bind_addr: String,
    // Chat completions (OpenRouter or any OpenAI-compatible API)
    pub llm_base_url: String,
    pub llm_api_key: Option<String>,
    pub llm_model: String,
    pub llm_max_tokens: u32,
    // Embeddings (OpenRouter doesn't serve embeddings, so this can point elsewhere,
    // e.g. OpenAI, Together, Jina, or a local server like Ollama/llama.cpp)
    pub embeddings_base_url: String,
    pub embeddings_api_key: Option<String>,
    pub embeddings_model: String,
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
        let llm_base_url = std::env::var("LLM_BASE_URL")
            .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into());
        let llm_api_key = std::env::var("LLM_API_KEY")
            .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
            .ok();
        Self {
            jwt_secret,
            access_ttl_minutes: std::env::var("ACCESS_TTL_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            refresh_ttl_days: std::env::var("REFRESH_TTL_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://rag.db?mode=rwc".into()),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into()),
            llm_model: std::env::var("LLM_MODEL")
                .or_else(|_| std::env::var("OPENROUTER_MODEL"))
                .unwrap_or_else(|_| "meta-llama/llama-3.3-70b-instruct".into()),
            llm_max_tokens: std::env::var("LLM_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1024),
            // Default to the same provider as chat; override only if your
            // chat provider doesn't serve embeddings (e.g. OpenRouter).
            embeddings_base_url: std::env::var("EMBEDDINGS_BASE_URL")
                .unwrap_or_else(|_| llm_base_url.clone()),
            embeddings_api_key: std::env::var("EMBEDDINGS_API_KEY")
                .ok()
                .or_else(|| llm_api_key.clone()),
            embeddings_model: std::env::var("EMBEDDINGS_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".into()),
            llm_base_url,
            llm_api_key,
        }
    }
}
