use std::{path::PathBuf, sync::Arc, time::Duration};

use fpdf::{Fpdf, ImageOptions, Orientation, PageSize, Pdf, Unit, UnitVec2};
use tokio::{
    sync::Semaphore,
    task::{self},
};
use tokio_util::sync::CancellationToken;

use crate::errors::{PdfError, PdfInputError, PdfInternalError};

use super::{
    layout::{CARD_SIZE_MM, Layout},
    storage::{GeneratedPdfCleaner, GeneratedPdfStorage, Running},
};

const DEFAULT_OUTPUT_DIR: &str = "./generated-pdf";
const MAX_PARALLEL_JOBS: usize = 4;
const CLEANUP_PERIOD: Duration = Duration::from_secs(10);
const FILE_TTL: Duration = Duration::from_mins(10);
const MAX_DIR_SIZE: u64 = 512 * 1024 * 1024;

pub(super) struct PdfGenerator {
    blocking_slots: Arc<Semaphore>,
    pdf_storage: GeneratedPdfStorage<Running>,
}

impl PdfGenerator {
    pub(super) fn new() -> Self {
        let pdf_storage = GeneratedPdfStorage::new(
            DEFAULT_OUTPUT_DIR,
            GeneratedPdfCleaner::new(FILE_TTL, MAX_DIR_SIZE),
            CLEANUP_PERIOD,
        )
        .start();

        Self {
            pdf_storage,
            blocking_slots: Arc::new(Semaphore::new(MAX_PARALLEL_JOBS)),
        }
    }

    pub(super) async fn generate_pdf(
        &self,
        cancellation_token: CancellationToken,
        layout: Layout,
        card_paths: &[PathBuf],
    ) -> Result<PathBuf, PdfError> {
        let permit = Arc::clone(&self.blocking_slots)
            .try_acquire_owned()
            .map_err(|_| PdfInputError::PdfGenerationBusy)?;

        let output_path = self.pdf_storage.next_output_path();
        let card_paths = card_paths.to_vec();

        let handle = task::spawn_blocking(move || -> Result<PathBuf, PdfError> {
            let _permit = permit;

            let mut pdf = create_pdf();
            render_cards(&mut pdf, &card_paths, layout, &cancellation_token)?;

            ensure_generation_not_cancelled(&cancellation_token)?;

            pdf.output_file_and_close(&output_path.to_string_lossy())
                .map_err(PdfInternalError::Pdf)
                .map_err(PdfError::from)?;

            Ok(output_path)
        });

        handle
            .await
            .map_err(PdfInternalError::from)
            .map_err(PdfError::from)?
    }

    #[cfg(test)]
    pub(super) fn for_tests(output_dir: PathBuf, permits: usize) -> Self {
        let pdf_storage = GeneratedPdfStorage::new(
            output_dir,
            GeneratedPdfCleaner::new(FILE_TTL, MAX_DIR_SIZE),
            CLEANUP_PERIOD,
        )
        .start();

        Self {
            pdf_storage,
            blocking_slots: Arc::new(Semaphore::new(permits)),
        }
    }
}

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

    for page in card_paths.chunks(layout.cards_per_page()) {
        ensure_generation_not_cancelled(cancellation_token)?;
        pdf.add_page();

        for (slot, card_path) in page.iter().enumerate() {
            let (x_mm, y_mm) = layout.position_for_slot(slot);
            let card_path = card_path.to_string_lossy();

            pdf.image(
                card_path.as_ref(),
                Unit::mm(x_mm),
                Unit::mm(y_mm),
                UnitVec2::mm(CARD_SIZE_MM.width(), CARD_SIZE_MM.height()),
                false,
                image_options.clone(),
                0,
                "",
            );
        }
    }

    Ok(())
}

fn ensure_generation_not_cancelled(cancellation_token: &CancellationToken) -> Result<(), PdfError> {
    if cancellation_token.is_cancelled() {
        return Err(PdfInputError::PdfGenerationCancelled.into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_generation_not_cancelled;
    use crate::errors::{PdfError, PdfInputError};

    #[test]
    fn ensure_generation_not_cancelled_returns_cancelled_when_token_is_already_cancelled() {
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();

        let error = ensure_generation_not_cancelled(&token).unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::PdfGenerationCancelled)
        ));
    }
}
