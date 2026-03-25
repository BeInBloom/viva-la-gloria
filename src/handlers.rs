use std::time::Duration;

use axum::{
    Json,
    extract::{Query, State},
};
use tokio_util::sync::CancellationToken;

use crate::{
    contracts::CardRepository,
    errors::{ListCardsError, PdfError, PdfInternalError},
    models::{AppState, GeneratePdfRequest, GeneratePdfResponse, ListCardsReq, ListCardsRes},
};

const PDF_GENERATION_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) async fn generate_pdf(
    State(state): State<AppState>,
    Json(payload): Json<GeneratePdfRequest>,
) -> Result<Json<GeneratePdfResponse>, PdfError> {
    let pdf_service = state.pdf_service;
    let card_ids = payload
        .card_ids
        .into_iter()
        .map(normalize_card_id)
        .collect::<Vec<_>>();

    let cancellation_token = CancellationToken::new();
    let _cancel_generation_on_drop = cancellation_token.clone().drop_guard();

    let handle = tokio::time::timeout(
        PDF_GENERATION_TIMEOUT,
        pdf_service.generate(&card_ids, cancellation_token.clone()),
    );

    let path = match handle.await {
        Ok(result) => result?,
        Err(_) => {
            cancellation_token.cancel();
            return Err(PdfInternalError::PdfGenerationTimedOut.into());
        }
    };

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

pub async fn list_cards(
    State(state): State<AppState>,
    Query(params): Query<ListCardsReq>,
) -> Result<Json<ListCardsRes>, ListCardsError> {
    let card_repo = state.card_repo;
    let cards = card_repo.list_card(params.into()).await?;
    Ok(Json(ListCardsRes { cards }))
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
