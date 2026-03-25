use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{card_repo::ManifestRepo, pdf::PdfGenerator};

const DEFAULT_CARD_PAGE_SIZE: usize = 24;
const MAX_CARD_PAGE_SIZE: usize = 100;

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
    pub after: Option<String>,
    pub limit: Option<usize>,
}

impl From<ListCardsReq> for ListCardsQuery {
    fn from(value: ListCardsReq) -> Self {
        Self {
            after: normalize_optional_card_id(value.after),
            limit: clamp_card_page_size(value.limit),
        }
    }
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

#[inline]
fn normalize_optional_card_id(after: Option<String>) -> Option<String> {
    let after = after?;
    let after = after.trim();

    if after.is_empty() {
        return None;
    }

    Some(format!("{after:0>3}"))
}

#[inline]
fn clamp_card_page_size(limit: Option<usize>) -> usize {
    match limit {
        Some(value @ 1..=MAX_CARD_PAGE_SIZE) => value,
        Some(value) if value > MAX_CARD_PAGE_SIZE => MAX_CARD_PAGE_SIZE,
        _ => DEFAULT_CARD_PAGE_SIZE,
    }
}

#[cfg(test)]
mod tests {
    use super::{ListCardsQuery, ListCardsReq};

    #[test]
    fn list_cards_query_uses_defaults() {
        let query: ListCardsQuery = ListCardsReq {
            after: None,
            limit: None,
        }
        .into();

        assert!(query.after.is_none());
        assert_eq!(query.limit, 24);
    }

    #[test]
    fn list_cards_query_normalizes_after_and_clamps_limit() {
        let query: ListCardsQuery = ListCardsReq {
            after: Some("7".to_owned()),
            limit: Some(999),
        }
        .into();

        assert_eq!(query.after.as_deref(), Some("007"));
        assert_eq!(query.limit, 100);
    }

    #[test]
    fn list_cards_query_treats_blank_after_as_missing() {
        let query: ListCardsQuery = ListCardsReq {
            after: Some("   ".to_owned()),
            limit: Some(0),
        }
        .into();

        assert!(query.after.is_none());
        assert_eq!(query.limit, 24);
    }

    #[test]
    fn list_cards_query_trims_after_before_normalizing() {
        let query: ListCardsQuery = ListCardsReq {
            after: Some(" 12 ".to_owned()),
            limit: Some(5),
        }
        .into();

        assert_eq!(query.after.as_deref(), Some("012"));
        assert_eq!(query.limit, 5);
    }
}
