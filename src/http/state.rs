use std::{net::IpAddr, sync::Arc};

use moka::future::Cache;

use crate::{repo::cards::ManifestRepo, service::pdf::PdfService};

#[derive(Clone)]
pub struct AppState {
    pub pdf_service: Arc<PdfService<ManifestRepo>>,
    pub card_repository: Arc<ManifestRepo>,
    pub pdf_rate_limit: Cache<IpAddr, ()>,
}
