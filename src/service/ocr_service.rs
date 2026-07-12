use std::path::Path;
use std::sync::OnceLock;

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

use crate::error::ApiError;

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
            "OCR models not loaded ({e}); download into {models_dir}"
        ))
    })
}

pub fn extract_text(models_dir: &str, image_bytes: &[u8]) -> Result<String, ApiError> {
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
