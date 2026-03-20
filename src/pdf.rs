use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use fpdf::{Fpdf, ImageOptions, Orientation, PageSize, Pdf, Unit, UnitVec2};
use tokio::{sync::Semaphore, task};

use crate::{
    contracts::CardRepository,
    errors::{PdfError, PdfInputError, PdfInternalError},
};

const DEFAULT_OUTPUT_DIR: &str = "./generated-pdf";
const PAGE_SIZE_MM: SizeMm = SizeMm::new(210.0, 297.0);
const CARD_SIZE_MM: SizeMm = SizeMm::new(63.0, 88.0);
const MAX_PARALLEL_JOBS: usize = 4;

pub struct PdfGenerator<R> {
    card_repository: R,
    output_dir: PathBuf,
    layout: Layout,
    blocking_slots: Arc<Semaphore>,
}

impl<R> PdfGenerator<R>
where
    R: CardRepository,
{
    pub fn new(card_repository: R) -> Self {
        Self {
            card_repository,
            output_dir: PathBuf::from(DEFAULT_OUTPUT_DIR),
            layout: Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM),
            blocking_slots: Arc::new(Semaphore::new(MAX_PARALLEL_JOBS)),
        }
    }

    pub async fn generate(&self, card_ids: &[String]) -> Result<PathBuf, PdfError> {
        ensure_cards_were_requested(card_ids)?;

        let card_paths = self.find_card_paths(card_ids).await?;

        let permit = Arc::clone(&self.blocking_slots)
            .try_acquire_owned()
            .map_err(|_| PdfInputError::PdfGenerationBusy)?;

        let output_dir = self.output_dir.clone();
        let output_path = make_output_path(&output_dir);
        let layout = self.layout;

        let handle = task::spawn_blocking(move || -> Result<PathBuf, PdfError> {
            let _permit = permit;

            std::fs::create_dir_all(&output_dir).map_err(|source| {
                PdfInternalError::CreateOutputDir {
                    path: output_dir.clone(),
                    source,
                }
            })?;

            let mut pdf = create_pdf();
            render_cards(&mut pdf, &card_paths, layout);

            let output_path_str = output_path.to_string_lossy();
            pdf.output_file_and_close(output_path_str.as_ref())
                .map_err(PdfInternalError::Pdf)
                .map_err(PdfError::from)?;

            Ok(output_path)
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

fn ensure_cards_were_requested(card_ids: &[String]) -> Result<(), PdfError> {
    if card_ids.is_empty() {
        return Err(PdfInputError::EmptyCardIds.into());
    }

    Ok(())
}

fn create_pdf() -> Fpdf<'static> {
    let mut pdf = Fpdf::new(Orientation::Portrait, PageSize::A4, "", UnitVec2::default());
    pdf.set_auto_page_break(false, Unit::zero());
    pdf.set_compression(true);
    pdf
}

fn render_cards(pdf: &mut Fpdf, card_paths: &[PathBuf], layout: Layout) {
    let image_options = ImageOptions {
        read_dpi: false,
        ..ImageOptions::default()
    };

    for page in card_paths.chunks(layout.cards_per_page) {
        pdf.add_page();

        for (slot, card_path) in page.iter().enumerate() {
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
}

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
