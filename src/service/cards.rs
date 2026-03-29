use std::sync::Arc;

use crate::{
    contracts::CardRepository,
    errors::ListCardsError,
    models::{ListCardsQuery, ListCardsRes},
};

pub struct CardsService<R> {
    card_repo: Arc<R>,
}

impl<R> CardsService<R>
where
    R: CardRepository,
{
    pub fn new(card_repo: Arc<R>) -> Self {
        Self { card_repo }
    }

    pub async fn list_cards(&self, query: ListCardsQuery) -> Result<ListCardsRes, ListCardsError> {
        self.card_repo.list_cards(query).await
    }
}
