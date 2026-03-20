use std::path::{Path, PathBuf};

use crate::{
    contracts::CardRepository,
    errors::CardRepositoryError,
    models::{AssetEntry, AssetVariant, Manifest},
};

pub(crate) struct ManifestRepo {
    manifest: Manifest,
}

impl ManifestRepo {
    pub fn new(manifest: Manifest) -> Self {
        Self { manifest }
    }

    fn find_card_path_by_id(&self, card_id: &str) -> Option<PathBuf> {
        let card = self.manifest.cards_by_id.get(card_id)?;
        let asset = select_asset(&card.assets)?;
        Some(build_asset_path(&self.manifest.asset_root, asset))
    }
}

impl CardRepository for ManifestRepo {
    async fn find_card_path_by_id(
        &self,
        card_id: &str,
    ) -> Result<Option<PathBuf>, CardRepositoryError> {
        Ok(self.find_card_path_by_id(card_id))
    }
}

fn select_asset(assets: &[AssetEntry]) -> Option<&AssetEntry> {
    assets
        .iter()
        .find(|asset| asset.variant == AssetVariant::Base)
        .or_else(|| assets.first())
}

fn build_asset_path(asset_root: &str, asset: &AssetEntry) -> PathBuf {
    let relative_path = Path::new(&asset.relative_path);
    let path = Path::new(asset_root).join(relative_path);

    match relative_path.file_name() {
        Some(file_name) if file_name == Path::new(&asset.filename).as_os_str() => path,
        _ => path.join(&asset.filename),
    }
}
