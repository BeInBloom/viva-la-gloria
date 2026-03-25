use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use fpdf::{Fpdf, ImageOptions, Orientation, PageSize, Pdf, Unit, UnitVec2};
use tokio::{sync::Semaphore, task};
use tokio_util::sync::CancellationToken;

use crate::{
    contracts::CardRepository,
    errors::{PdfError, PdfInputError, PdfInternalError},
};

const DEFAULT_OUTPUT_DIR: &str = "./generated-pdf";
const PAGE_SIZE_MM: SizeMm = SizeMm::new(210.0, 297.0);
const CARD_SIZE_MM: SizeMm = SizeMm::new(63.0, 88.0);
const MAX_PARALLEL_JOBS: usize = 4;

pub struct PdfGenerator<R> {
    card_repository: Arc<R>,
    output_dir: PathBuf,
    layout: Layout,
    blocking_slots: Arc<Semaphore>,
}

impl<R> PdfGenerator<R>
where
    R: CardRepository,
{
    pub fn new(card_repository: Arc<R>) -> Self {
        Self {
            card_repository,
            output_dir: PathBuf::from(DEFAULT_OUTPUT_DIR),
            layout: Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM),
            blocking_slots: Arc::new(Semaphore::new(MAX_PARALLEL_JOBS)),
        }
    }

    pub async fn generate(
        &self,
        card_ids: &[String],
        cancellation_token: CancellationToken,
    ) -> Result<PathBuf, PdfError> {
        ensure_cards_were_requested(card_ids)?;

        let card_paths = self.find_card_paths(card_ids).await?;

        let permit = Arc::clone(&self.blocking_slots)
            .try_acquire_owned()
            .map_err(|_| PdfInputError::PdfGenerationBusy)?;

        let output_dir = self.output_dir.clone();
        let output_path = make_output_path(&output_dir);
        let temp_output_path = make_temporary_output_path(&output_path);
        let layout = self.layout;

        let handle = task::spawn_blocking(move || -> Result<PathBuf, PdfError> {
            //Передаем owned для drop
            let _permit = permit;

            let result = (|| -> Result<PathBuf, PdfError> {
                fs::create_dir_all(&output_dir).map_err(|source| {
                    PdfInternalError::CreateOutputDir {
                        path: output_dir.clone(),
                        source,
                    }
                })?;

                let mut pdf = create_pdf();
                render_cards(&mut pdf, &card_paths, layout, &cancellation_token)?;
                ensure_generation_not_cancelled(&cancellation_token)?;

                let temp_output_path_str = temp_output_path.to_string_lossy();
                pdf.output_file_and_close(temp_output_path_str.as_ref())
                    .map_err(PdfInternalError::Pdf)
                    .map_err(PdfError::from)?;

                ensure_generation_not_cancelled(&cancellation_token)?;

                fs::rename(&temp_output_path, &output_path)
                    .map_err(|source| PdfInternalError::PersistGeneratedPdf {
                        path: output_path.clone(),
                        source,
                    })
                    .map_err(PdfError::from)?;

                Ok(output_path.clone())
            })();

            if result.is_err() {
                remove_file_if_exists(&temp_output_path);
            }

            result
        });

        handle
            .await
            .map_err(PdfInternalError::from)
            .map_err(PdfError::from)?
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

#[inline]
fn ensure_cards_were_requested(card_ids: &[String]) -> Result<(), PdfError> {
    if card_ids.is_empty() {
        return Err(PdfInputError::EmptyCardIds.into());
    }

    Ok(())
}

#[inline]
fn create_pdf() -> Fpdf<'static> {
    let mut pdf = Fpdf::new(Orientation::Portrait, PageSize::A4, "", UnitVec2::default());
    pdf.set_auto_page_break(false, Unit::zero());
    pdf.set_compression(true);
    pdf
}

fn render_cards(
    pdf: &mut Fpdf,
    card_paths: &[PathBuf],
    layout: Layout,
    cancellation_token: &CancellationToken,
) -> Result<(), PdfError> {
    let image_options = ImageOptions {
        read_dpi: false,
        ..ImageOptions::default()
    };

    for page in card_paths.chunks(layout.cards_per_page) {
        ensure_generation_not_cancelled(cancellation_token)?;
        pdf.add_page();

        for (slot, card_path) in page.iter().enumerate() {
            // ensure_generation_not_cancelled(cancellation_token)?;
            let (x_mm, y_mm) = layout.position_for_slot(slot);
            let card_path = card_path.to_string_lossy();

            pdf.image(
                card_path.as_ref(),
                Unit::mm(x_mm),
                Unit::mm(y_mm),
                UnitVec2::mm(CARD_SIZE_MM.width, CARD_SIZE_MM.height),
                false,
                image_options.clone(),
                0,
                "",
            );
        }
    }

    Ok(())
}

#[inline]
fn ensure_generation_not_cancelled(cancellation_token: &CancellationToken) -> Result<(), PdfError> {
    if cancellation_token.is_cancelled() {
        return Err(PdfInputError::PdfGenerationCancelled.into());
    }

    Ok(())
}

#[inline]
fn make_temporary_output_path(output_path: &Path) -> PathBuf {
    output_path.with_extension("pdf.part")
}

#[inline]
fn remove_file_if_exists(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

#[inline]
fn make_output_path(output_dir: &Path) -> PathBuf {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    output_dir.join(format!("cards-{timestamp_ms}.pdf"))
}

#[derive(Debug, Clone, Copy)]
struct SizeMm {
    width: f64,
    height: f64,
}

impl SizeMm {
    const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy)]
struct Layout {
    cards_per_row: usize,
    cards_per_page: usize,
    margin_left_mm: f64,
    margin_top_mm: f64,
}

impl Layout {
    fn new(page: SizeMm, card: SizeMm) -> Self {
        let cards_per_row = (page.width / card.width).floor() as usize;
        let cards_per_column = (page.height / card.height).floor() as usize;
        let used_width = cards_per_row as f64 * card.width;
        let used_height = cards_per_column as f64 * card.height;

        Self {
            cards_per_row,
            cards_per_page: cards_per_row * cards_per_column,
            margin_left_mm: (page.width - used_width) / 2.0,
            margin_top_mm: (page.height - used_height) / 2.0,
        }
    }

    fn position_for_slot(&self, slot: usize) -> (f64, f64) {
        let row = slot / self.cards_per_row;
        let column = slot % self.cards_per_row;

        (
            self.margin_left_mm + column as f64 * CARD_SIZE_MM.width,
            self.margin_top_mm + row as f64 * CARD_SIZE_MM.height,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{CARD_SIZE_MM, Layout, PAGE_SIZE_MM, PdfGenerator, ensure_cards_were_requested};
    use crate::{
        contracts::CardRepository,
        errors::{CardRepositoryError, ListCardsError, PdfError, PdfInputError},
        models::{ListCardsQuery, ListCardsRes},
    };
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::Semaphore;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn ensure_cards_were_requested_rejects_empty_input() {
        let error = ensure_cards_were_requested(&[]).unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::EmptyCardIds)
        ));
    }

    #[test]
    fn layout_for_a4_cards_uses_expected_grid_and_margins() {
        let layout = Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM);

        assert_eq!(layout.cards_per_row, 3);
        assert_eq!(layout.cards_per_page, 9);
        assert_eq!(layout.margin_left_mm, 10.5);
        assert_eq!(layout.margin_top_mm, 16.5);
    }

    #[test]
    fn layout_positions_center_card_grid_slots() {
        let layout = Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM);

        assert_eq!(layout.position_for_slot(0), (10.5, 16.5));
        assert_eq!(layout.position_for_slot(4), (73.5, 104.5));
        assert_eq!(layout.position_for_slot(8), (136.5, 192.5));
    }

    #[tokio::test]
    async fn find_card_paths_returns_missing_ids_in_request_order() {
        let generator = test_generator(
            StubRepo::new([
                ("001", Some("tests/001.jpeg")),
                ("002", None),
                ("003", Some("tests/003.jpeg")),
                ("004", None),
            ]),
            1,
            unique_test_path("missing-paths"),
        );

        let error = generator
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
        let generator = test_generator(
            StubRepo::new([("001", Some("tests/001.jpeg"))]),
            0,
            unique_test_path("busy"),
        );

        let error = generator
            .generate(&["001".to_owned()], CancellationToken::new())
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::PdfGenerationBusy)
        ));
    }

    #[tokio::test]
    async fn generate_returns_cancelled_when_token_is_already_cancelled() {
        let output_dir = unique_test_path("cancelled");
        let generator = test_generator(
            StubRepo::new([("001", Some("tests/001.jpeg"))]),
            1,
            output_dir.clone(),
        );
        let cancellation_token = CancellationToken::new();
        cancellation_token.cancel();

        let error = generator
            .generate(&["001".to_owned()], cancellation_token)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::PdfGenerationCancelled)
        ));

        let _ = std::fs::remove_dir_all(output_dir);
    }

    fn test_generator(
        repo: StubRepo,
        permits: usize,
        output_dir: PathBuf,
    ) -> PdfGenerator<StubRepo> {
        PdfGenerator {
            card_repository: Arc::new(repo),
            output_dir,
            layout: Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM),
            blocking_slots: Arc::new(Semaphore::new(permits)),
        }
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
