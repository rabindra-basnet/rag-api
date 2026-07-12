use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct Config {
    pub environment: String,
    pub jwt_secret: String,
    pub refresh_jwt_secret: String,
    pub access_ttl_minutes: i64,
    pub refresh_ttl_days: i64,
    pub cookie_secure: bool,
    pub trust_proxy: bool,
    pub signup_login: bool,
    pub cors_allow_origin: String,
    pub cors_expose_headers: String,
    pub allowed_file_extensions: String,
    pub auth_rate_per_second: u64,
    pub auth_rate_burst: u32,
    pub api_rate_per_second: u64,
    pub api_rate_burst: u32,
    pub database_url: String,
    pub bind_addr: String,
    pub upload_dir: String,
    pub llm_base_url: String,
    pub llm_api_key: Option<String>,
    pub llm_model: String,
    pub llm_max_tokens: u32,
    pub ocr_models_dir: String,
    pub embeddings_base_url: String,
    pub embeddings_api_key: Option<String>,
    pub embeddings_model: String,
    pub tokenizer_model: String,
    pub chunk_max_tokens: usize,
    pub chunk_overlap_tokens: usize,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn init() {
    dotenvy::dotenv().ok();
    let cfg = Config::from_env();
    CONFIG.set(cfg).expect("config initialized twice");
}

pub fn get() -> &'static Config {
    CONFIG.get().expect("config not initialized — call parameter::init() first")
}

pub fn try_get() -> Option<&'static Config> {
    CONFIG.get()
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| std::env::var(k).ok())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_flag(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v == "true" || v == "1")
        .unwrap_or(default)
}

impl Config {
    fn from_env() -> Self {
        let environment = env_or("ENVIRONMENT", "development");
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!("JWT_SECRET not set — generating ephemeral secret");
            use rand::RngCore;
            let mut buf = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut buf);
            hex::encode(buf)
        });
        let refresh_jwt_secret = std::env::var("REFRESH_JWT_SECRET").unwrap_or_else(|_| {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(
                format!("{jwt_secret}:refresh-v1").as_bytes(),
            ))
        });
        let llm_base_url = env_or("LLM_BASE_URL", "https://openrouter.ai/api/v1");
        let llm_api_key = env_first(&["LLM_API_KEY", "OPENROUTER_API_KEY"]);
        Self {
            environment,
            jwt_secret,
            refresh_jwt_secret,
            access_ttl_minutes: env_parse("ACCESS_TTL_MINUTES", 15),
            refresh_ttl_days: env_parse("REFRESH_TTL_DAYS", 30),
            cookie_secure: env_flag("COOKIE_SECURE", true),
            trust_proxy: env_flag("TRUST_PROXY", false),
            signup_login: env_flag("SIGNUP_LOGIN", false),
            cors_allow_origin: env_or("CORS_ALLOW_ORIGIN", "*"),
            cors_expose_headers: env_or("CORS_EXPOSE_HEADERS", ""),
            allowed_file_extensions: env_or(
                "ALLOWED_FILE_EXTENSIONS",
                ".txt,.md,.csv,.json,.pdf,.png,.jpg,.jpeg,.webp,.gif,.xml,.html",
            ),
            auth_rate_per_second: env_parse("AUTH_RATE_PER_SECOND", 1),
            auth_rate_burst: env_parse("AUTH_RATE_BURST", 1),
            api_rate_per_second: env_parse("API_RATE_PER_SECOND", 2),
            api_rate_burst: env_parse("API_RATE_BURST", 5),
            database_url: env_or("DATABASE_URL", "sqlite://rag.db?mode=rwc"),
            bind_addr: env_or("BIND_ADDR", "0.0.0.0:3000"),
            upload_dir: env_or("UPLOAD_DIR", "./uploads"),
            llm_model: env_first(&["LLM_MODEL", "OPENROUTER_MODEL"])
                .unwrap_or_else(|| "meta-llama/llama-3.3-70b-instruct".into()),
            llm_max_tokens: env_parse("LLM_MAX_TOKENS", 1024),
            ocr_models_dir: env_or("OCR_MODELS_DIR", "./models"),
            embeddings_base_url: env_or("EMBEDDINGS_BASE_URL", &llm_base_url),
            embeddings_api_key: env_first(&["EMBEDDINGS_API_KEY"]).or_else(|| llm_api_key.clone()),
            embeddings_model: env_or("EMBEDDINGS_MODEL", "text-embedding-3-small"),
            tokenizer_model: env_or("TOKENIZER_MODEL", "gpt2"),
            chunk_max_tokens: env_parse("CHUNK_MAX_TOKENS", 512),
            chunk_overlap_tokens: env_parse("CHUNK_OVERLAP_TOKENS", 64),
            llm_base_url,
            llm_api_key,
        }
    }
}
