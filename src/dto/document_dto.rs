use serde::Deserialize;
use validator::Validate;

#[derive(Deserialize, Validate)]
pub struct IngestReq {
    #[validate(length(min = 1, max = 500, message = "title must be 1-500 characters"))]
    pub title: String,
    #[validate(length(min = 1, message = "content is empty"))]
    pub content: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum IngestBody {
    One(IngestReq),
    Many(Vec<IngestReq>),
}

#[derive(Deserialize)]
pub struct TextIngestQuery {
    pub title: Option<String>,
}

#[derive(Deserialize, Validate)]
pub struct ChatReq {
    #[validate(length(min = 1, max = 4000, message = "question must be 1-4000 characters"))]
    pub question: String,
    #[validate(range(min = 1, max = 20, message = "top_k must be 1-20"))]
    #[serde(default)]
    pub top_k: Option<usize>,
}
