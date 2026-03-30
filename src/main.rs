mod app;
mod contracts;
mod errors;
mod http;
mod models;
mod repo;
mod service;

const SERVER_ADDR: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let app = app::build_app().await?;

    let listener = tokio::net::TcpListener::bind(SERVER_ADDR).await?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
