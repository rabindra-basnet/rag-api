use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
};
use async_openai::types::embeddings::{CreateEmbeddingRequestArgs, EmbeddingInput};

use crate::error::ApiError;
use crate::state::AppState;

pub struct ChatMessage {
    pub role: &'static str,
    pub content: String,
}

fn to_openai_message(m: ChatMessage) -> Result<ChatCompletionRequestMessage, ApiError> {
    let msg = match m.role {
        "system" => ChatCompletionRequestSystemMessageArgs::default()
            .content(m.content)
            .build()
            .map(ChatCompletionRequestMessage::System),
        _ => ChatCompletionRequestUserMessageArgs::default()
            .content(m.content)
            .build()
            .map(ChatCompletionRequestMessage::User),
    };
    msg.map_err(|e| {
        tracing::error!(error = %e, "message build failure");
        ApiError::Internal("llm request build failure".into())
    })
}

pub async fn chat_completion(
    state: &AppState,
    messages: Vec<ChatMessage>,
) -> Result<String, ApiError> {
    let messages = messages
        .into_iter()
        .map(to_openai_message)
        .collect::<Result<Vec<_>, _>>()?;

    let request = CreateChatCompletionRequestArgs::default()
        .model(&state.cfg.llm_model)
        .max_completion_tokens(state.cfg.llm_max_tokens)
        .messages(messages)
        .build()
        .map_err(|e| {
            tracing::error!(error = %e, "chat request build failure");
            ApiError::Internal("llm request build failure".into())
        })?;

    let response = state.llm.chat().create(request).await?;
    response
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or_else(|| ApiError::Internal("llm returned no content".into()))
}

/// Batch-embed texts via an OpenAI-compatible /embeddings endpoint.
pub async fn embed(state: &AppState, texts: &[String]) -> Result<Vec<Vec<f32>>, ApiError> {
    let request = CreateEmbeddingRequestArgs::default()
        .model(&state.cfg.embeddings_model)
        .input(EmbeddingInput::StringArray(texts.to_vec()))
        .build()
        .map_err(|e| {
            tracing::error!(error = %e, "embedding request build failure");
            ApiError::Internal("embeddings request build failure".into())
        })?;

    let response = state.embeddings.embeddings().create(request).await?;
    if response.data.len() != texts.len() {
        return Err(ApiError::Internal("embeddings count mismatch".into()));
    }
    Ok(response.data.into_iter().map(|d| d.embedding).collect())
}
