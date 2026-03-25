mod card_repo;
mod contracts;
mod errors;
mod handlers;
mod models;
mod pdf;

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use tower_http::services::{ServeDir, ServeFile};

use crate::{
    card_repo::ManifestRepo,
    handlers::{generate_pdf, list_cards},
    models::{AppState, Manifest},
    pdf::PdfGenerator,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let manifest: Manifest =
        serde_json::from_str(&tokio::fs::read_to_string("./manifest.json").await?)?;
    let manifest_repo = Arc::new(ManifestRepo::new(manifest));
    let pdf_service = PdfGenerator::new(manifest_repo.clone());

    let state = AppState {
        pdf_service: Arc::new(pdf_service),
        card_repo: manifest_repo,
    };

    let app = Router::new()
        .route_service("/", ServeFile::new("static/index.html"))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/generated-pdf", ServeDir::new("generated-pdf"))
        .nest_service("/previews", ServeDir::new("assets/previews"))
        .route("/pdf", post(generate_pdf))
        .route("/cards", get(list_cards))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;

    axum::serve(listener, app).await?;
    Ok(())
}
