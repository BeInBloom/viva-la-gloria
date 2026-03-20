use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use fpdf::{Fpdf, FpdfError, ImageOptions, Orientation, PageSize, Pdf, Unit, UnitVec2};
use thiserror::Error;

use crate::models::{AssetEntry, AssetVariant, Manifest};

const OUTPUT_DIR: &str = "./generated-pdf";
const PAGE_SIZE_MM: SizeMm = SizeMm::new(210.0, 297.0);
const CARD_SIZE_MM: SizeMm = SizeMm::new(63.0, 88.0);

pub fn generate_pdf(manifest: &Manifest, card_ids: &[String]) -> Result<PathBuf, PdfError> {
    if card_ids.is_empty() {
        return Err(PdfError::BadRequest(PdfInputError::EmptyCardIds));
    }

    let output_dir = Path::new(OUTPUT_DIR);
    fs::create_dir_all(output_dir).map_err(|error| {
        PdfError::Internal(PdfInternalError::CreateOutputDir {
            path: output_dir.into(),
            source: error,
        })
    })?;

    let layout = Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM)?;
    let mut pdf = Fpdf::new(Orientation::Portrait, PageSize::A4, "", UnitVec2::default());

    pdf.set_auto_page_break(false, Unit::zero());
    pdf.set_compression(true);

    let image_options = ImageOptions {
        read_dpi: false,
        ..ImageOptions::default()
    };

    for page_cards in card_ids.chunks(layout.cards_per_page) {
        pdf.add_page();

        for (slot, card_id) in page_cards.iter().enumerate() {
            let asset_path = resolve_asset_path(manifest, card_id)?;
            let (x_mm, y_mm) = layout.position_for_slot(slot);
            let asset_path_str = asset_path.to_string_lossy();

            pdf.image(
                asset_path_str.as_ref(),
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

    let output_path = make_output_path(output_dir);
    let output_path_str = output_path.to_string_lossy();

    pdf.output_file_and_close(output_path_str.as_ref())
        .map_err(|error| PdfError::Internal(PdfInternalError::Pdf(error)))?;

    Ok(output_path)
}

fn resolve_asset_path(manifest: &Manifest, card_id: &str) -> Result<PathBuf, PdfError> {
    let card = manifest.cards_by_id.get(card_id).ok_or_else(|| {
        PdfError::BadRequest(PdfInputError::CardNotFound {
            card_id: card_id.to_owned(),
        })
    })?;
    let asset = select_asset(card_id, &card.assets)?;
    let full_path = build_asset_path(&manifest.asset_root, asset);

    if !full_path.is_file() {
        return Err(PdfError::Internal(PdfInternalError::AssetFileNotFound {
            card_id: card_id.to_owned(),
            path: full_path,
        }));
    }

    Ok(full_path)
}

fn select_asset<'a>(
    card_id: &str,
    assets: &'a [AssetEntry],
) -> Result<&'a AssetEntry, PdfInputError> {
    assets
        .iter()
        .find(|asset| asset.variant == AssetVariant::Base)
        .or_else(|| assets.first())
        .ok_or_else(|| PdfInputError::NoAssetsForCard {
            card_id: card_id.to_owned(),
        })
}

fn build_asset_path(asset_root: &str, asset: &AssetEntry) -> PathBuf {
    let relative_path = Path::new(&asset.relative_path);
    let path = Path::new(asset_root).join(relative_path);

    match relative_path.file_name() {
        Some(file_name) if file_name == Path::new(&asset.filename).as_os_str() => path,
        _ => path.join(&asset.filename),
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
    fn new(page: SizeMm, card: SizeMm) -> Result<Self, PdfError> {
        if card.width <= 0.0 || card.height <= 0.0 {
            return Err(PdfError::BadRequest(PdfInputError::InvalidCardSize {
                width: card.width,
                height: card.height,
            }));
        }

        if card.width > page.width || card.height > page.height {
            return Err(PdfError::BadRequest(PdfInputError::CardTooLarge {
                card_width: card.width,
                card_height: card.height,
                page_width: page.width,
                page_height: page.height,
            }));
        }

        let cards_per_row = (page.width / card.width).floor() as usize;
        let cards_per_column = (page.height / card.height).floor() as usize;
        let used_width = cards_per_row as f64 * card.width;
        let used_height = cards_per_column as f64 * card.height;

        Ok(Self {
            cards_per_row,
            cards_per_page: cards_per_row * cards_per_column,
            margin_left_mm: (page.width - used_width) / 2.0,
            margin_top_mm: (page.height - used_height) / 2.0,
        })
    }

    fn position_for_slot(&self, slot: usize) -> (f64, f64) {
        let row = slot / self.cards_per_row;
        let column = slot % self.cards_per_row;
        let x = self.margin_left_mm + column as f64 * CARD_SIZE_MM.width;
        let y = self.margin_top_mm + row as f64 * CARD_SIZE_MM.height;

        (x, y)
    }
}

#[derive(Debug, Error)]
pub enum PdfError {
    #[error(transparent)]
    BadRequest(#[from] PdfInputError),

    #[error(transparent)]
    Internal(#[from] PdfInternalError),
}

#[derive(Debug, Error)]
pub enum PdfInputError {
    #[error("card id list is empty")]
    EmptyCardIds,

    #[error("card '{card_id}' not found in manifest")]
    CardNotFound { card_id: String },

    #[error("card '{card_id}' does not contain any assets")]
    NoAssetsForCard { card_id: String },

    #[error("invalid card size: {width}x{height} mm")]
    InvalidCardSize { width: f64, height: f64 },

    #[error(
        "card is too large for page: card={card_width}x{card_height} mm, page={page_width}x{page_height} mm"
    )]
    CardTooLarge {
        card_width: f64,
        card_height: f64,
        page_width: f64,
        page_height: f64,
    },
}

#[derive(Debug, Error)]
pub enum PdfInternalError {
    #[error("asset file for card '{card_id}' not found at '{path}'")]
    AssetFileNotFound { card_id: String, path: PathBuf },

    #[error("failed to create output dir '{path}'")]
    CreateOutputDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("pdf generation failed")]
    Pdf(#[source] FpdfError),
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::build_asset_path;
    use crate::models::{AssetEntry, AssetVariant};

    fn asset_entry(relative_path: &str, filename: &str) -> AssetEntry {
        AssetEntry {
            variant: AssetVariant::Base,
            variant_index: None,
            asset_revision: Some("rev-01".to_owned()),
            processing_profile: "default".to_owned(),
            faceai: false,
            filename: filename.to_owned(),
            relative_path: relative_path.to_owned(),
        }
    }

    #[test]
    fn build_asset_path_supports_directory_relative_paths() {
        let asset = asset_entry("set_2", "123__flameheart__variant-base__rev-01.jpeg");

        let path = build_asset_path("assets/images/eoj/main_sets", &asset);

        assert_eq!(
            path,
            Path::new("assets/images/eoj/main_sets")
                .join("set_2")
                .join("123__flameheart__variant-base__rev-01.jpeg")
        );
    }

    #[test]
    fn build_asset_path_supports_file_relative_paths() {
        let asset = asset_entry(
            "set_2/123__flameheart__variant-base__rev-01.jpeg",
            "123__flameheart__variant-base__rev-01.jpeg",
        );

        let path = build_asset_path("assets/images/eoj/main_sets", &asset);

        assert_eq!(
            path,
            Path::new("assets/images/eoj/main_sets")
                .join("set_2/123__flameheart__variant-base__rev-01.jpeg")
        );
    }
}
