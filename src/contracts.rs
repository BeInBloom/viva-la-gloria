use std::path::PathBuf;

use crate::{
    errors::{CardRepositoryError, ListCardsError},
    models::{CardPreview, ListCardsQuery},
};

pub trait CardRepository: Send + Sync + 'static {
    async fn find_card_path_by_id(
        &self,
        card_id: &str,
    ) -> Result<Option<PathBuf>, CardRepositoryError>;

    async fn list_card(&self, query: ListCardsQuery) -> Result<Vec<CardPreview>, ListCardsError>;
}
