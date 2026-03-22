use std::path::PathBuf;

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
            Self::BadRequest(PdfInputError::PdfGenerationCancelled) => {
                StatusCode::REQUEST_TIMEOUT
            }
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal(PdfInternalError::PdfGenerationTimedOut) => StatusCode::REQUEST_TIMEOUT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum PdfInputError {
    #[error("card id list is empty")]
    EmptyCardIds,

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

    #[error("failed to create output dir '{path}'")]
    CreateOutputDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to move generated pdf into '{path}'")]
    PersistGeneratedPdf {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("pdf generation timed out")]
    PdfGenerationTimedOut,

    #[error("generated pdf path is missing file name")]
    GeneratedPdfFileNameMissing,

    #[error("pdf generation task failed")]
    PdfTaskFailed(#[from] tokio::task::JoinError),

    #[error("pdf generation failed")]
    Pdf(#[source] FpdfError),
}
