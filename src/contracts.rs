use std::path::PathBuf;

use crate::{
    errors::{CardRepositoryError, ListCardsError},
    models::{ListCardsQuery, ListCardsRes},
};

pub trait CardRepository: Send + Sync + 'static {
    async fn find_card_path_by_id(
        &self,
        card_id: &str,
    ) -> Result<Option<PathBuf>, CardRepositoryError>;

    async fn list_cards(&self, query: ListCardsQuery) -> Result<ListCardsRes, ListCardsError>;
}
