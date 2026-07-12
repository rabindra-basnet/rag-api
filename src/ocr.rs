//! Local image OCR with the pure-Rust `ocrs` engine (no tesseract, no
//! system dependencies, no network calls). Needs the two .rten model
//! files in OCR_MODELS_DIR:
//!
//!   curl -o models/text-detection.rten \
//!     https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten
//!   curl -o models/text-recognition.rten \
//!     https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten

use std::path::Path;
use std::sync::OnceLock;

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

use crate::error::ApiError;

/// The engine is expensive to build (loads two ML models), so it's created
/// once and reused. OcrEngine is Send + Sync.
static ENGINE: OnceLock<Result<OcrEngine, String>> = OnceLock::new();

fn engine(models_dir: &str) -> Result<&'static OcrEngine, ApiError> {
    let result = ENGINE.get_or_init(|| {
        let dir = Path::new(models_dir);
        let detection = Model::load_file(dir.join("text-detection.rten"))
            .map_err(|e| format!("text-detection.rten: {e}"))?;
        let recognition = Model::load_file(dir.join("text-recognition.rten"))
            .map_err(|e| format!("text-recognition.rten: {e}"))?;
        OcrEngine::new(OcrEngineParams {
            detection_model: Some(detection),
            recognition_model: Some(recognition),
            ..Default::default()
        })
        .map_err(|e| format!("ocr engine init: {e}"))
    });
    result.as_ref().map_err(|e| {
        tracing::error!(error = %e, models_dir, "OCR models unavailable");
        ApiError::Internal(format!(
            "OCR models not loaded ({e}); download the ocrs .rten models into {models_dir} — see src/ocr.rs"
        ))
    })
}

/// Decode an image and run text detection + recognition. Blocking CPU work:
/// call via spawn_blocking.
pub fn extract_text(models_dir: &str, image_bytes: &[u8]) -> Result<String, ApiError> {
    // Decode first: a bad upload is the client's error (400) whether or not
    // the OCR models are installed.
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| ApiError::BadRequest(format!("invalid image: {e}")))?
        .into_rgb8();

    let engine = engine(models_dir)?;

    let source = ImageSource::from_bytes(img.as_raw(), img.dimensions())
        .map_err(|e| ApiError::Internal(format!("ocr input: {e}")))?;
    let input = engine
        .prepare_input(source)
        .map_err(|e| ApiError::Internal(format!("ocr prepare: {e}")))?;

    let text = engine
        .get_text(&input)
        .map_err(|e| ApiError::Internal(format!("ocr run: {e}")))?;

    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(ApiError::BadRequest("no text found in image".into()));
    }
    Ok(text)
}
