use axum::{
    Json,
    extract::{Query, State},
};

use crate::{
    contracts::CardRepository,
    errors::ListCardsError,
    http::{dto::ListCardsReq, state::AppState},
    models::ListCardsRes,
};

pub async fn list_cards(
    State(state): State<AppState>,
    Query(params): Query<ListCardsReq>,
) -> Result<Json<ListCardsRes>, ListCardsError> {
    let page = state.card_repository.list_cards(params.into()).await?;
    Ok(Json(page))
}
