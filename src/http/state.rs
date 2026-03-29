use std::sync::Arc;

use crate::{
    repo::cards::ManifestRepo,
    service::{cards::CardsService, pdf::PdfService},
};

#[derive(Clone)]
pub struct AppState {
    pub pdf_service: Arc<PdfService<ManifestRepo>>,
    pub cards_service: Arc<CardsService<ManifestRepo>>,
}
