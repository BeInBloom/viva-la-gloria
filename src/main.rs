mod card_repo;
mod contracts;
mod errors;
mod handlers;
mod models;
mod pdf;

use std::sync::Arc;

use axum::{Router, routing::post};
use tower_http::services::{ServeDir, ServeFile};

use crate::{
    card_repo::ManifestRepo,
    handlers::generate_pdf,
    models::{AppState, Manifest},
    pdf::PdfGenerator,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let manifest: Manifest =
        serde_json::from_str(&tokio::fs::read_to_string("./manifest.json").await?)?;
    let mainfest_repo = ManifestRepo::new(manifest);
    let pdf_service = PdfGenerator::new(mainfest_repo);

    let state = AppState {
        pdf_service: Arc::new(pdf_service),
    };

    let app = Router::new()
        .route_service("/", ServeFile::new("static/index.html"))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/generated-pdf", ServeDir::new("generated-pdf"))
        .route("/pdf", post(generate_pdf))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;

    axum::serve(listener, app).await?;
    Ok(())
}
