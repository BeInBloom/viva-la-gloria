use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub asset_root: String,
    pub preview_root: String,
    pub cards_by_id: BTreeMap<String, CardManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardManifestEntry {
    pub set_name: String,
    pub card_id: String,
    pub title_slug: String,
    pub preview_relative_path: Option<String>,
    pub review_flags: Vec<String>,
    pub assets: Vec<AssetEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetEntry {
    pub variant: AssetVariant,
    pub variant_index: Option<String>,
    pub asset_revision: Option<String>,
    pub processing_profile: String,
    pub faceai: bool,
    pub filename: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetVariant {
    Base,
    Blank,
    Text,
    Promo,
    Phantom,
}

pub struct ListCardsQuery {
    pub after: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Serialize)]
pub struct ListCardsRes {
    pub items: Vec<CardPreview>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CardPreview {
    pub card_id: String,
    pub title: String,
    pub preview_url: Option<String>,
}
