use std::{io, path::PathBuf, sync::Arc, time::Duration};

use crate::{
    contracts::CardRepository,
    errors::{PdfError, PdfInputError, PdfInternalError},
    http::dto::normalize_card_id,
};

use super::{
    generator::PdfGenerator,
    layout::{CARD_SIZE_MM, Layout, PAGE_SIZE_MM},
};

const PDF_GENERATION_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CARD_IDS_PER_PDF: usize = 100;

pub struct PdfService<R> {
    card_repository: Arc<R>,
    pdf_generator: PdfGenerator,
}

impl<R> PdfService<R>
where
    R: CardRepository,
{
    pub fn new(card_repository: Arc<R>) -> io::Result<Self> {
        Ok(Self::with_pdf_generator(
            card_repository,
            PdfGenerator::new()?,
        ))
    }

    fn with_pdf_generator(card_repository: Arc<R>, pdf_generator: PdfGenerator) -> Self {
        Self {
            card_repository,
            pdf_generator,
        }
    }

    pub async fn generate(&self, requested_card_ids: Vec<String>) -> Result<PathBuf, PdfError> {
        let card_ids = requested_card_ids
            .into_iter()
            .map(normalize_card_id)
            .collect::<Vec<_>>();

        validate_card_count(&card_ids)?;

        let card_paths = self.find_card_paths(&card_ids).await?;
        let layout = Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM);

        let handle = tokio::time::timeout(
            PDF_GENERATION_TIMEOUT,
            self.pdf_generator.generate_pdf(layout, &card_paths),
        );

        match handle.await {
            Ok(result) => result,
            Err(_) => Err(PdfInternalError::PdfGenerationTimedOut.into()),
        }
    }

    async fn find_card_paths(&self, card_ids: &[String]) -> Result<Vec<PathBuf>, PdfError> {
        let mut paths = Vec::with_capacity(card_ids.len());
        let mut missing_card_ids = Vec::new();

        for card_id in card_ids {
            match self.card_repository.find_card_path_by_id(card_id).await? {
                Some(path) => paths.push(path),
                None => missing_card_ids.push(card_id.clone()),
            }
        }

        if !missing_card_ids.is_empty() {
            return Err(PdfInputError::CardsNotFound {
                card_ids: missing_card_ids,
            }
            .into());
        }

        Ok(paths)
    }
}

fn validate_card_count(card_ids: &[String]) -> Result<(), PdfError> {
    let len = card_ids.len();

    if len == 0 {
        return Err(PdfInputError::EmptyCardIds.into());
    }

    if len > MAX_CARD_IDS_PER_PDF {
        return Err(PdfInputError::TooManyCardIds {
            limit: MAX_CARD_IDS_PER_PDF,
            actual: len,
        }
        .into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        contracts::CardRepository,
        errors::{CardRepositoryError, ListCardsError, PdfError, PdfInputError},
        models::{ListCardsQuery, ListCardsRes},
    };

    use super::super::generator::PdfGenerator;
    use super::{PdfService, validate_card_count};

    #[test]
    fn validate_card_count_rejects_empty_input() {
        let error = validate_card_count(&[]).unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::EmptyCardIds)
        ));
    }

    #[test]
    fn validate_card_count_rejects_inputs_larger_than_limit() {
        let card_ids = (0..101).map(|index| index.to_string()).collect::<Vec<_>>();

        let error = validate_card_count(&card_ids).unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::TooManyCardIds {
                limit: 100,
                actual: 101,
            })
        ));
    }

    #[tokio::test]
    async fn find_card_paths_returns_missing_ids_in_request_order() {
        let service = test_service(
            StubRepo::new([
                ("001", Some("tests/001.jpeg")),
                ("002", None),
                ("003", Some("tests/003.jpeg")),
                ("004", None),
            ]),
            1,
            unique_test_path("missing-paths"),
        );

        let error = service
            .find_card_paths(&["002".to_owned(), "001".to_owned(), "004".to_owned()])
            .await
            .unwrap_err();

        match error {
            PdfError::BadRequest(PdfInputError::CardsNotFound { card_ids }) => {
                assert_eq!(card_ids, vec!["002".to_owned(), "004".to_owned()]);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn generate_returns_busy_when_no_blocking_slots_are_available() {
        let service = test_service(
            StubRepo::new([("001", Some("tests/001.jpeg"))]),
            0,
            unique_test_path("busy"),
        );

        let error = service.generate(vec!["001".to_owned()]).await.unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::PdfGenerationBusy)
        ));
    }

    fn test_service(repo: StubRepo, permits: usize, output_dir: PathBuf) -> PdfService<StubRepo> {
        PdfService::with_pdf_generator(Arc::new(repo), PdfGenerator::for_tests(output_dir, permits))
    }

    fn unique_test_path(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("eoj-card-generator-{name}-{timestamp}"))
    }

    struct StubRepo {
        card_paths: BTreeMap<String, Option<PathBuf>>,
    }

    impl StubRepo {
        fn new<const N: usize>(card_paths: [(&str, Option<&str>); N]) -> Self {
            Self {
                card_paths: card_paths
                    .into_iter()
                    .map(|(card_id, path)| (card_id.to_owned(), path.map(PathBuf::from)))
                    .collect(),
            }
        }
    }

    impl CardRepository for StubRepo {
        async fn find_card_path_by_id(
            &self,
            card_id: &str,
        ) -> Result<Option<PathBuf>, CardRepositoryError> {
            Ok(self.card_paths.get(card_id).cloned().flatten())
        }

        async fn list_cards(&self, _query: ListCardsQuery) -> Result<ListCardsRes, ListCardsError> {
            Ok(ListCardsRes {
                items: Vec::new(),
                next_cursor: None,
            })
        }
    }
}
