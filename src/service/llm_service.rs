use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::parameter::Config;
use crate::error::ApiError;
use crate::error::llm_error::LlmError;

#[derive(Serialize)]
pub struct ChatMessage {
    pub role: &'static str,
    pub content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(110))
        .build()
        .expect("reqwest client")
}

use std::sync::OnceLock;
static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(http_client)
}

fn auth_header(key: &Option<String>) -> Result<String, ApiError> {
    key.as_deref()
        .map(|k| format!("Bearer {k}"))
        .ok_or_else(|| ApiError::Llm(LlmError::ApiKeyNotConfigured))
}

async fn post_json<T: serde::de::DeserializeOwned>(
    base_url: &str,
    api_key: &Option<String>,
    path: &str,
    body: serde_json::Value,
    what: &str,
) -> Result<T, ApiError> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let resp = client()
        .post(&url)
        .header("Authorization", auth_header(api_key)?)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::error!(%status, body, "{what} upstream error");
        return Err(ApiError::Llm(LlmError::UpstreamError(format!("{what} upstream error"))));
    }
    Ok(resp.json().await?)
}

pub async fn chat_completion(cfg: &Config, messages: Vec<ChatMessage>) -> Result<String, ApiError> {
    let parsed: ChatResponse = post_json(
        &cfg.llm_base_url,
        &cfg.llm_api_key,
        "/chat/completions",
        json!({
            "model": cfg.llm_model,
            "max_tokens": cfg.llm_max_tokens,
            "messages": messages,
        }),
        "chat completion",
    )
    .await?;

    parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or_else(|| ApiError::Llm(LlmError::NoContent))
}

pub async fn embed(cfg: &Config, texts: &[String]) -> Result<Vec<Vec<f32>>, ApiError> {
    let parsed: EmbeddingsResponse = post_json(
        &cfg.embeddings_base_url,
        &cfg.embeddings_api_key,
        "/embeddings",
        json!({
            "model": cfg.embeddings_model,
            "input": texts,
        }),
        "embeddings",
    )
    .await?;

    if parsed.data.len() != texts.len() {
        return Err(ApiError::Llm(LlmError::EmbeddingsCountMismatch));
    }
    Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
}
