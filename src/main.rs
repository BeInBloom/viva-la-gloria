mod app;
mod contracts;
mod errors;
mod http;
mod models;
mod repo;
mod service;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = app::build_app().await?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;

    axum::serve(listener, app).await?;
    Ok(())
}
