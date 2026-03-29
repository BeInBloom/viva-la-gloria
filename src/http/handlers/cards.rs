use axum::{
    Json,
    extract::{Query, State},
};

use crate::{
    errors::ListCardsError,
    http::{dto::ListCardsReq, state::AppState},
    models::ListCardsRes,
};

pub async fn list_cards(
    State(state): State<AppState>,
    Query(params): Query<ListCardsReq>,
) -> Result<Json<ListCardsRes>, ListCardsError> {
    let page = state.cards_service.list_cards(params.into()).await?;
    Ok(Json(page))
}
