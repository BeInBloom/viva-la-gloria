use axum::{Json, extract::State};

use crate::{
    errors::{PdfError, PdfInternalError},
    models::{AppState, GeneratePdfRequest, GeneratePdfResponse},
};

pub async fn generate_pdf(
    State(state): State<AppState>,
    Json(payload): Json<GeneratePdfRequest>,
) -> Result<Json<GeneratePdfResponse>, PdfError> {
    let card_ids = payload
        .card_ids
        .into_iter()
        .map(normalize_card_id)
        .collect::<Vec<_>>();

    let path = state
        .pdf_service
        .generate(&card_ids)
        .await
        .map_err(|_| PdfInternalError::PdfGenerationTimedOut)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(PdfInternalError::GeneratedPdfFileNameMissing)?;

    Ok(Json(GeneratePdfResponse {
        path: format!("/generated-pdf/{file_name}"),
    }))
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
