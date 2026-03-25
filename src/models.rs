use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{card_repo::ManifestRepo, pdf::PdfGenerator};

#[derive(Clone)]
pub struct AppState {
    pub pdf_service: Arc<PdfGenerator<ManifestRepo>>,
    pub card_repo: Arc<ManifestRepo>,
}

#[derive(Debug, Deserialize)]
pub struct GeneratePdfRequest {
    pub card_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListCardsReq {
    pub cursor: String,
    pub limit: usize,
}

impl From<ListCardsReq> for ListCardsQuery {
    fn from(value: ListCardsReq) -> Self {
        Self {
            cursor: format!("{:0>3}", value.cursor),
            limin: value.limit,
        }
    }
}

pub struct ListCardsQuery {
    pub cursor: String,
    pub limin: usize,
}

#[derive(Debug, Serialize)]
pub struct ListCardsRes {
    pub cards: Vec<CardPreview>,
}

#[derive(Debug, Serialize)]
pub struct CardPreview {
    pub id: String,
    pub card_name: String,
    pub preview_path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GeneratePdfResponse {
    pub path: String,
}

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
