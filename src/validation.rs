use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::Json;
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::error::ApiError;

/// Json extractor that (1) turns hyper/axum body rejections (bad
/// content-type, malformed JSON, over-limit body) into clean JSON 400s and
/// (2) runs `validator` field rules on the deserialized value.
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|e: JsonRejection| ApiError::BadRequest(e.body_text()))?;
        value.validate().map_err(|e| {
            let msg = e
                .field_errors()
                .into_iter()
                .map(|(field, errs)| {
                    let detail = errs
                        .first()
                        .and_then(|err| err.message.as_ref())
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "invalid".into());
                    format!("{field}: {detail}")
                })
                .collect::<Vec<_>>()
                .join("; ");
            ApiError::BadRequest(msg)
        })?;
        Ok(Self(value))
    }
}
