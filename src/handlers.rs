use std::time::Duration;

use axum::{Json, extract::State, http::StatusCode};
use tokio::time::timeout;

use crate::{
    models::{AppState, GeneratePdfRequest, GeneratePdfResponse},
    pdf::{PdfError, generate_pdf as build_pdf},
};

const TIMEOUT_FOR_PDF_GENERATE: Duration = Duration::from_secs(2);

pub async fn generate_pdf(
    State(state): State<AppState>,
    Json(payload): Json<GeneratePdfRequest>,
) -> Result<Json<GeneratePdfResponse>, (StatusCode, String)> {
    let manifest = state.manifest;
    let card_ids: Vec<String> = payload
        .card_ids
        .into_iter()
        .map(normalize_card_id)
        .collect();

    let handle = tokio::task::spawn_blocking(move || build_pdf(&manifest, &card_ids));
    let path = timeout(TIMEOUT_FOR_PDF_GENERATE, handle)
        .await
        .map_err(|_| internal_error("pdf generation timed out"))?
        .map_err(|error| internal_error(format!("pdf task failed: {error}")))?
        .map_err(pdf_error_response)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| internal_error("generated pdf path is missing file name"))?;

    Ok(Json(GeneratePdfResponse {
        path: format!("/generated-pdf/{file_name}"),
    }))
}

#[inline]
fn pdf_error_response(error: PdfError) -> (StatusCode, String) {
    match error {
        PdfError::BadRequest(error) => (StatusCode::BAD_REQUEST, error.to_string()),
        PdfError::Internal(error) => internal_error(error.to_string()),
    }
}

#[inline]
fn internal_error(message: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, message.into())
}

#[inline]
fn normalize_card_id(card_id: String) -> String {
    let card_id = card_id.trim();
    format!("{card_id:0>3}")
}

#[cfg(test)]
mod tests {
    use super::normalize_card_id;

    #[test]
    fn normalize_card_id_adds_missing_leading_zeroes() {
        assert_eq!(normalize_card_id("1".to_owned()), "001");
        assert_eq!(normalize_card_id("12".to_owned()), "012");
        assert_eq!(normalize_card_id("123".to_owned()), "123");
    }

    #[test]
    fn normalize_card_id_only_normalizes_width() {
        assert_eq!(normalize_card_id("ab".to_owned()), "0ab");
        assert_eq!(normalize_card_id("abc".to_owned()), "abc");
    }
}
