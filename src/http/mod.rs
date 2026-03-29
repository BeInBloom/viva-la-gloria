pub mod dto;
pub mod handlers;
pub mod state;

use axum::Router;

use crate::http::{
    handlers::{cards::list_cards, pdf::generate_pdf},
    state::AppState,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/pdf", axum::routing::post(generate_pdf))
        .route("/cards", axum::routing::get(list_cards))
        .with_state(state)
}
