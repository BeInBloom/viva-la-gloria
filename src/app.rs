use std::{net::IpAddr, sync::Arc, time::Duration};

use axum::Router;
use moka::future::Cache;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::{
    http::{router::router, state::AppState},
    models::Manifest,
    repo::cards::ManifestRepo,
    service::pdf::PdfService,
};

const IP_TTL: Duration = Duration::from_secs(10);
const MAX_CACHE_SIZE: u64 = 1_000;

pub async fn build_app() -> anyhow::Result<Router> {
    let manifest = load_manifest("./manifest.json").await?;
    let manifest_repo = Arc::new(ManifestRepo::new(manifest)?);
    let pdf_service = Arc::new(PdfService::new(manifest_repo.clone())?);
    let ip_cache = get_ip_cache();

    let state = AppState {
        pdf_service,
        card_repository: manifest_repo,
        pdf_rate_limit: ip_cache,
    };

    Ok(Router::new()
        .route_service("/", ServeFile::new("static/index.html"))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/generated-pdf", ServeDir::new("generated-pdf"))
        .nest_service("/previews", ServeDir::new("assets/previews"))
        .merge(router(state))
        .layer(TraceLayer::new_for_http()))
}

async fn load_manifest(path: &str) -> anyhow::Result<Manifest> {
    Ok(serde_json::from_str(
        &tokio::fs::read_to_string(path).await?,
    )?)
}

fn get_ip_cache() -> Cache<IpAddr, ()> {
    Cache::builder()
        .time_to_live(IP_TTL)
        .max_capacity(MAX_CACHE_SIZE)
        .build()
}
