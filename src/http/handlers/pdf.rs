use axum::{Json, extract::State};

use crate::{
    errors::{PdfError, PdfInternalError},
    http::{
        dto::{GeneratePdfRequest, GeneratePdfResponse},
        state::AppState,
    },
};

pub(crate) async fn generate_pdf(
    State(state): State<AppState>,
    Json(payload): Json<GeneratePdfRequest>,
) -> Result<Json<GeneratePdfResponse>, PdfError> {
    let path = state.pdf_service.generate(payload.card_ids).await?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(PdfInternalError::GeneratedPdfFileNameMissing)?;

    Ok(Json(GeneratePdfResponse {
        path: format!("/generated-pdf/{file_name}"),
    }))
}
