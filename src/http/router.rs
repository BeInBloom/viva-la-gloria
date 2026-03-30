use std::time::Duration;

use axum::{
    BoxError, Router, error_handling::HandleErrorLayer, http::StatusCode,
    middleware::from_fn_with_state, routing::post,
};
use tower::ServiceBuilder;

use crate::http::{
    handlers::{cards::list_cards, pdf::generate_pdf},
    middleware::rate_limit::rate_limit_by_ip,
    state::AppState,
};

const PDF_GENERATION_TIMEOUT: Duration = Duration::from_secs(2);

pub fn router(state: AppState) -> Router {
    let pdf_generate_route = Router::new().route(
        "/pdf",
        post(generate_pdf)
            .route_layer(
                ServiceBuilder::new()
                    .layer(HandleErrorLayer::new(handle_timeout_error))
                    .timeout(PDF_GENERATION_TIMEOUT),
            )
            .route_layer(from_fn_with_state(state.clone(), rate_limit_by_ip)),
    );

    Router::new()
        .merge(pdf_generate_route)
        .route("/cards", axum::routing::get(list_cards))
        .with_state(state)
}

async fn handle_timeout_error(error: BoxError) -> (StatusCode, String) {
    if error.is::<tower::timeout::error::Elapsed>() {
        return (StatusCode::REQUEST_TIMEOUT, "request timed out".to_string());
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("internal middleware error: {error}"),
    )
}
