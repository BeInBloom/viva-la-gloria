use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use fpdf::FpdfError;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum CardRepositoryError {}

#[derive(Debug, Error)]
pub(crate) enum PdfError {
    #[error(transparent)]
    BadRequest(#[from] PdfInputError),

    #[error(transparent)]
    Internal(#[from] PdfInternalError),
}

impl From<CardRepositoryError> for PdfError {
    fn from(value: CardRepositoryError) -> Self {
        Self::Internal(PdfInternalError::CardRepository(value))
    }
}

impl IntoResponse for PdfError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let message = self.to_string();

        (status, message).into_response()
    }
}

impl PdfError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(PdfInputError::PdfGenerationBusy) => StatusCode::TOO_MANY_REQUESTS,
            Self::BadRequest(PdfInputError::PdfGenerationCancelled) => StatusCode::REQUEST_TIMEOUT,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal(PdfInternalError::PdfGenerationTimedOut) => StatusCode::REQUEST_TIMEOUT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum PdfInputError {
    #[error("card id list must not be empty")]
    EmptyCardIds,

    #[error("card id list is too large: maximum is {limit}, got {actual}")]
    TooManyCardIds { limit: usize, actual: usize },

    #[error("pdf generation is busy, try again later")]
    PdfGenerationBusy,

    #[error("pdf generation was cancelled")]
    PdfGenerationCancelled,

    #[error("cards not found: {card_ids:?}")]
    CardsNotFound { card_ids: Vec<String> },
    // #[error("card '{card_id}' does not contain any assets")]
    // NoAssetsForCard { card_id: String },
    //
    // #[error("invalid card size: {width}x{height} mm")]
    // InvalidCardSize { width: f64, height: f64 },
    // #[error(
    //     "card is too large for page: card={card_width}x{card_height} mm, page={page_width}x{page_height} mm"
    // )]
    // CardTooLarge {
    //     card_width: f64,
    //     card_height: f64,
    //     page_width: f64,
    //     page_height: f64,
    // },
}

#[derive(Debug, Error)]
pub(crate) enum PdfInternalError {
    // #[error("asset file for card '{card_id}' not found at '{path}'")]
    // AssetFileNotFound { card_id: String, path: PathBuf },
    #[error("card repository error")]
    CardRepository(#[from] CardRepositoryError),

    #[error("pdf generation timed out")]
    PdfGenerationTimedOut,

    #[error("generated pdf path is missing file name")]
    GeneratedPdfFileNameMissing,

    #[error("pdf generation task failed")]
    PdfTaskFailed(#[from] tokio::task::JoinError),

    #[error("pdf generation failed")]
    Pdf(#[source] FpdfError),
}

#[derive(Debug, Error)]
pub enum ListCardsError {}

impl IntoResponse for ListCardsError {
    fn into_response(self) -> Response {
        (StatusCode::NO_CONTENT, "no content").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{PdfError, PdfInputError, PdfInternalError};
    use axum::http::StatusCode;

    #[test]
    fn pdf_error_maps_empty_input_to_bad_request() {
        let error = PdfError::from(PdfInputError::EmptyCardIds);

        assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(error.to_string(), "card id list must not be empty");
    }

    #[test]
    fn pdf_error_maps_too_many_card_ids_to_bad_request() {
        let error = PdfError::from(PdfInputError::TooManyCardIds {
            limit: 100,
            actual: 101,
        });

        assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(
            error.to_string(),
            "card id list is too large: maximum is 100, got 101"
        );
    }

    #[test]
    fn pdf_error_maps_busy_to_too_many_requests() {
        let error = PdfError::from(PdfInputError::PdfGenerationBusy);

        assert_eq!(error.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(error.to_string(), "pdf generation is busy, try again later");
    }

    #[test]
    fn pdf_error_maps_cancelled_to_request_timeout() {
        let error = PdfError::from(PdfInputError::PdfGenerationCancelled);

        assert_eq!(error.status_code(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(error.to_string(), "pdf generation was cancelled");
    }

    #[test]
    fn pdf_error_preserves_missing_card_ids_in_message() {
        let error = PdfError::from(PdfInputError::CardsNotFound {
            card_ids: vec!["003".to_owned(), "005".to_owned()],
        });

        assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(error.to_string(), "cards not found: [\"003\", \"005\"]");
    }

    #[test]
    fn pdf_error_maps_internal_timeout_to_request_timeout() {
        let error = PdfError::from(PdfInternalError::PdfGenerationTimedOut);

        assert_eq!(error.status_code(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(error.to_string(), "pdf generation timed out");
    }
}
