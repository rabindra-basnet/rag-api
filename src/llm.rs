//! OpenAI-compatible API client (OpenRouter, OpenAI, Ollama, ...) over
//! plain reqwest — chat completions and embeddings.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

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

fn auth_header(key: &Option<String>) -> Result<String, ApiError> {
    key.as_deref()
        .map(|k| format!("Bearer {k}"))
        .ok_or_else(|| ApiError::Internal("LLM api key not configured".into()))
}

async fn post_json<T: serde::de::DeserializeOwned>(
    state: &AppState,
    base_url: &str,
    api_key: &Option<String>,
    path: &str,
    body: serde_json::Value,
    what: &str,
) -> Result<T, ApiError> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let resp = state
        .http
        .post(&url)
        .header("Authorization", auth_header(api_key)?)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::error!(%status, body, "{what} upstream error");
        return Err(ApiError::Internal(format!("{what} upstream error")));
    }
    Ok(resp.json().await?)
}

pub async fn chat_completion(
    state: &AppState,
    messages: Vec<ChatMessage>,
) -> Result<String, ApiError> {
    let parsed: ChatResponse = post_json(
        state,
        &state.cfg.llm_base_url,
        &state.cfg.llm_api_key,
        "/chat/completions",
        json!({
            "model": state.cfg.llm_model,
            "max_tokens": state.cfg.llm_max_tokens,
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
        .ok_or_else(|| ApiError::Internal("llm returned no content".into()))
}

/// OCR an image with a vision-capable model: the image is sent inline as a
/// base64 data URL in the OpenAI-compatible multimodal message format.
pub async fn image_to_text(
    state: &AppState,
    mime: &str,
    image: &[u8],
) -> Result<String, ApiError> {
    use base64::Engine;
    let data_url = format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(image)
    );

    let parsed: ChatResponse = post_json(
        state,
        &state.cfg.llm_base_url,
        &state.cfg.llm_api_key,
        "/chat/completions",
        json!({
            "model": state.cfg.vision_model,
            "max_tokens": state.cfg.llm_max_tokens,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text",
                      "text": "Transcribe ALL text visible in this image, preserving reading order and structure. Output only the transcribed text, no commentary. If the image contains no text, output exactly: NO_TEXT" },
                    { "type": "image_url", "image_url": { "url": data_url } }
                ]
            }],
        }),
        "image ocr",
    )
    .await?;

    let text = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or_else(|| ApiError::Internal("vision model returned no content".into()))?;

    let text = text.trim().to_string();
    if text == "NO_TEXT" || text.is_empty() {
        return Err(ApiError::BadRequest("no text found in image".into()));
    }
    Ok(text)
}

/// Batch-embed texts via an OpenAI-compatible /embeddings endpoint.
pub async fn embed(state: &AppState, texts: &[String]) -> Result<Vec<Vec<f32>>, ApiError> {
    let parsed: EmbeddingsResponse = post_json(
        state,
        &state.cfg.embeddings_base_url,
        &state.cfg.embeddings_api_key,
        "/embeddings",
        json!({
            "model": state.cfg.embeddings_model,
            "input": texts,
        }),
        "embeddings",
    )
    .await?;

    if parsed.data.len() != texts.len() {
        return Err(ApiError::Internal("embeddings count mismatch".into()));
    }
    Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
}
