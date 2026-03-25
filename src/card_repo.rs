use std::{
    ops::Bound,
    path::{Path, PathBuf},
};

use crate::{
    contracts::CardRepository,
    errors::{CardRepositoryError, ListCardsError},
    models::{
        AssetEntry, AssetVariant, CardManifestEntry, CardPreview, ListCardsQuery, ListCardsRes,
        Manifest,
    },
};

const DEFAULT_STATR: &str = "000";

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

    fn list_cards_by_query(&self, query: ListCardsQuery) -> ListCardsRes {
        let mut items = self.collect_card_previews(&query);
        let has_more = items.len() > query.limit;

        if has_more {
            items.truncate(query.limit);
        }

        let next_cursor = has_more
            .then(|| items.last().map(|item| item.card_id.clone()))
            .flatten();

        ListCardsRes { items, next_cursor }
    }

    fn collect_card_previews(&self, query: &ListCardsQuery) -> Vec<CardPreview> {
        match query.after.as_deref() {
            Some(after) => self.collect_card(after, query.limit),
            None => self.collect_card(DEFAULT_STATR, query.limit),
        }
    }

    fn collect_card(&self, after: &str, limit: usize) -> Vec<CardPreview> {
        self.manifest
            .cards_by_id
            .range::<str, _>((Bound::Excluded(after), Bound::Unbounded))
            .take(limit + 1)
            .map(|(_id, entry)| card_preview_from_card_entry(&self.manifest.preview_root, entry))
            .collect()
    }
}

impl CardRepository for ManifestRepo {
    async fn find_card_path_by_id(
        &self,
        card_id: &str,
    ) -> Result<Option<PathBuf>, CardRepositoryError> {
        Ok(self.find_card_path_by_id(card_id))
    }

    async fn list_cards(&self, query: ListCardsQuery) -> Result<ListCardsRes, ListCardsError> {
        Ok(self.list_cards_by_query(query))
    }
}

#[inline]
fn select_asset(assets: &[AssetEntry]) -> Option<&AssetEntry> {
    assets
        .iter()
        .find(|asset| asset.variant == AssetVariant::Base)
        .or_else(|| assets.first())
}

#[inline]
fn build_asset_path(asset_root: &str, asset: &AssetEntry) -> PathBuf {
    let relative_path = Path::new(&asset.relative_path);
    let path = Path::new(asset_root).join(relative_path);

    match relative_path.file_name() {
        Some(file_name) if file_name == Path::new(&asset.filename).as_os_str() => path,
        _ => path.join(&asset.filename),
    }
}

fn card_preview_from_card_entry(preview_root: &str, entry: &CardManifestEntry) -> CardPreview {
    CardPreview {
        card_id: entry.card_id.clone(),
        title: entry.title_slug.clone(),
        preview_url: build_preview_url(preview_root, entry.preview_relative_path.as_deref()),
    }
}

fn build_preview_url(preview_root: &str, preview_relative_path: Option<&str>) -> Option<String> {
    let preview_relative_path = preview_relative_path?;
    let preview_root_suffix = Path::new(preview_root)
        .strip_prefix("assets/previews")
        .ok()?;
    let preview_public_path = if preview_root_suffix.as_os_str().is_empty() {
        PathBuf::from(preview_relative_path)
    } else {
        preview_root_suffix.join(preview_relative_path)
    };

    Some(format!(
        "/previews/{}",
        preview_public_path.to_string_lossy().replace('\\', "/")
    ))
}

#[cfg(test)]
mod tests {
    use super::{ManifestRepo, build_preview_url};
    use crate::models::{AssetEntry, AssetVariant, CardManifestEntry, ListCardsQuery, Manifest};
    use std::{collections::BTreeMap, path::Path};

    #[test]
    fn collect_card_previews_excludes_after_cursor_and_peeks_one_extra() {
        let repo = test_repo();
        let query = ListCardsQuery {
            after: Some("001".to_owned()),
            limit: 1,
        };

        let items = repo.collect_card_previews(&query);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].card_id, "002");
        assert_eq!(items[1].card_id, "003");
    }

    #[test]
    fn list_cards_by_query_returns_first_page_with_next_cursor() {
        let repo = test_repo();
        let query = ListCardsQuery {
            after: None,
            limit: 2,
        };

        let page = repo.list_cards_by_query(query);

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].card_id, "001");
        assert_eq!(page.items[1].card_id, "002");
        assert_eq!(page.next_cursor.as_deref(), Some("002"));
    }

    #[test]
    fn list_cards_by_query_returns_last_page_without_next_cursor() {
        let repo = test_repo();
        let query = ListCardsQuery {
            after: Some("002".to_owned()),
            limit: 2,
        };

        let page = repo.list_cards_by_query(query);

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].card_id, "003");
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn list_cards_by_query_keeps_missing_preview_as_none() {
        let repo = ManifestRepo::new(Manifest {
            asset_root: "assets/images/eoj/main_sets".to_owned(),
            preview_root: "assets/previews/eoj/main_sets".to_owned(),
            cards_by_id: BTreeMap::from([("001".to_owned(), test_card_without_preview("001"))]),
        });

        let page = repo.list_cards_by_query(ListCardsQuery {
            after: None,
            limit: 1,
        });

        assert_eq!(page.items.len(), 1);
        assert!(page.items[0].preview_url.is_none());
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn find_card_path_by_id_prefers_base_asset() {
        let repo = ManifestRepo::new(Manifest {
            asset_root: "assets/images/eoj/main_sets".to_owned(),
            preview_root: "assets/previews/eoj/main_sets".to_owned(),
            cards_by_id: BTreeMap::from([(
                "001".to_owned(),
                CardManifestEntry {
                    set_name: "set_1".to_owned(),
                    card_id: "001".to_owned(),
                    title_slug: "flame-magus".to_owned(),
                    preview_relative_path: Some("set_1/001.jpeg".to_owned()),
                    review_flags: Vec::new(),
                    assets: vec![
                        test_asset(
                            AssetVariant::Promo,
                            "001__promo.jpeg",
                            "set_1/001__promo.jpeg",
                        ),
                        test_asset(AssetVariant::Base, "001__base.jpeg", "set_1/001__base.jpeg"),
                    ],
                },
            )]),
        });

        let path = repo.find_card_path_by_id("001");

        assert_eq!(
            path,
            Some(Path::new("assets/images/eoj/main_sets").join("set_1/001__base.jpeg"))
        );
    }

    #[test]
    fn find_card_path_by_id_falls_back_to_first_asset_and_appends_filename() {
        let repo = ManifestRepo::new(Manifest {
            asset_root: "assets/images/eoj/main_sets".to_owned(),
            preview_root: "assets/previews/eoj/main_sets".to_owned(),
            cards_by_id: BTreeMap::from([(
                "015".to_owned(),
                CardManifestEntry {
                    set_name: "set_1".to_owned(),
                    card_id: "015".to_owned(),
                    title_slug: "warden-hilda".to_owned(),
                    preview_relative_path: None,
                    review_flags: Vec::new(),
                    assets: vec![test_asset(
                        AssetVariant::Blank,
                        "015__warden-hilda__variant-blank__rev-01.jpeg",
                        "set_1/015",
                    )],
                },
            )]),
        });

        let path = repo.find_card_path_by_id("015");

        assert_eq!(
            path,
            Some(
                Path::new("assets/images/eoj/main_sets")
                    .join("set_1/015")
                    .join("015__warden-hilda__variant-blank__rev-01.jpeg"),
            )
        );
    }

    #[test]
    fn build_preview_url_uses_public_previews_prefix() {
        let url = build_preview_url(
            "assets/previews/eoj/main_sets",
            Some("set_1/001__flame-magus__variant-base__rev-02.jpeg"),
        );

        assert_eq!(
            url.as_deref(),
            Some("/previews/eoj/main_sets/set_1/001__flame-magus__variant-base__rev-02.jpeg")
        );
    }

    #[test]
    fn build_preview_url_returns_none_when_preview_is_missing() {
        let url = build_preview_url("assets/previews/eoj/main_sets", None);

        assert!(url.is_none());
    }

    #[test]
    fn build_preview_url_returns_none_for_non_public_preview_root() {
        let url = build_preview_url(
            "assets/private-previews/eoj/main_sets",
            Some("set_1/001__flame-magus__variant-base__rev-02.jpeg"),
        );

        assert!(url.is_none());
    }

    fn test_repo() -> ManifestRepo {
        let cards_by_id = BTreeMap::from([
            ("001".to_owned(), test_card("001")),
            ("002".to_owned(), test_card("002")),
            ("003".to_owned(), test_card("003")),
        ]);

        ManifestRepo::new(Manifest {
            asset_root: "assets/images/eoj/main_sets".to_owned(),
            preview_root: "assets/previews/eoj/main_sets".to_owned(),
            cards_by_id,
        })
    }

    fn test_card(card_id: &str) -> CardManifestEntry {
        CardManifestEntry {
            set_name: "set_1".to_owned(),
            card_id: card_id.to_owned(),
            title_slug: format!("card-{card_id}"),
            preview_relative_path: Some(format!("set_1/{card_id}.jpeg")),
            review_flags: Vec::new(),
            assets: Vec::new(),
        }
    }

    fn test_card_without_preview(card_id: &str) -> CardManifestEntry {
        CardManifestEntry {
            set_name: "set_1".to_owned(),
            card_id: card_id.to_owned(),
            title_slug: format!("card-{card_id}"),
            preview_relative_path: None,
            review_flags: Vec::new(),
            assets: Vec::new(),
        }
    }

    fn test_asset(variant: AssetVariant, filename: &str, relative_path: &str) -> AssetEntry {
        AssetEntry {
            variant,
            variant_index: None,
            asset_revision: None,
            processing_profile: "test-profile".to_owned(),
            faceai: false,
            filename: filename.to_owned(),
            relative_path: relative_path.to_owned(),
        }
    }
}
