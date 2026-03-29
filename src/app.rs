use std::sync::Arc;

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::{
    http::{self, state::AppState},
    models::Manifest,
    repo::cards::ManifestRepo,
    service::{cards::CardsService, pdf::PdfService},
};

pub async fn build_app() -> anyhow::Result<Router> {
    let manifest = load_manifest("./manifest.json").await?;
    let manifest_repo = Arc::new(ManifestRepo::new(manifest));
    let pdf_service = Arc::new(PdfService::new(manifest_repo.clone()));
    let cards_service = Arc::new(CardsService::new(manifest_repo));

    let state = AppState {
        pdf_service,
        cards_service,
    };

    Ok(Router::new()
        .route_service("/", ServeFile::new("static/index.html"))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/generated-pdf", ServeDir::new("generated-pdf"))
        .nest_service("/previews", ServeDir::new("assets/previews"))
        .merge(http::router(state)))
}

pub async fn load_manifest(path: &str) -> anyhow::Result<Manifest> {
    Ok(serde_json::from_str(
        &tokio::fs::read_to_string(path).await?,
    )?)
}
