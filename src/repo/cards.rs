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

const DEFAULT_START: &str = "000";
const PUBLIC_PREVIEWS_ROOT: &str = "assets/previews";
const PUBLIC_PREVIEWS_URL_PREFIX: &str = "/previews";

#[derive(Debug, Clone)]
struct PreviewUrlBuilder {
    public_root: PathBuf,
}

impl PreviewUrlBuilder {
    fn new(preview_root: &str) -> Self {
        let public_root = Path::new(preview_root)
            .strip_prefix(PUBLIC_PREVIEWS_ROOT)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| {
                panic!(
                    "invalid manifest.preview_root '{preview_root}': expected path under '{PUBLIC_PREVIEWS_ROOT}'"
                )
            });

        Self { public_root }
    }

    fn build(&self, preview_relative_path: &str) -> String {
        let public_path = self.public_root.join(preview_relative_path);
        format!(
            "{PUBLIC_PREVIEWS_URL_PREFIX}/{}",
            public_path.to_string_lossy()
        )
    }
}

#[derive(Debug, Clone)]
struct AssetPathBuilder {
    root: PathBuf,
}

impl AssetPathBuilder {
    fn new(asset_root: &str) -> Self {
        Self {
            root: PathBuf::from(asset_root),
        }
    }

    fn build(&self, asset: &AssetEntry) -> PathBuf {
        let relative_path = Path::new(&asset.relative_path);
        let path = self.root.join(relative_path);

        match relative_path.file_name() {
            Some(file_name) if file_name == Path::new(&asset.filename).as_os_str() => path,
            _ => path.join(&asset.filename),
        }
    }
}

pub(crate) struct ManifestRepo {
    manifest: Manifest,
    asset_path_builder: AssetPathBuilder,
    preview_url_builder: PreviewUrlBuilder,
}

impl ManifestRepo {
    pub fn new(manifest: Manifest) -> Self {
        let asset_path_builder = AssetPathBuilder::new(&manifest.asset_root);
        let preview_url_builder = PreviewUrlBuilder::new(&manifest.preview_root);
        Self {
            manifest,
            asset_path_builder,
            preview_url_builder,
        }
    }

    fn find_card_path(&self, card_id: &str) -> Option<PathBuf> {
        let card = self.manifest.cards_by_id.get(card_id)?;
        let asset = select_asset(&card.assets)?;
        Some(self.asset_path_builder.build(asset))
    }

    fn list_cards_page(&self, query: ListCardsQuery) -> ListCardsRes {
        let mut items = self.collect_card_previews(&query);
        let has_more = items.len() > query.limit;

        if has_more {
            items.truncate(query.limit);
        }

        let next_cursor = items
            .last()
            .filter(|_| has_more)
            .map(|item| item.card_id.clone());

        ListCardsRes { items, next_cursor }
    }

    fn collect_card_previews(&self, query: &ListCardsQuery) -> Vec<CardPreview> {
        let after = query.after.as_deref().unwrap_or(DEFAULT_START);
        self.collect_cards(after, query.limit)
    }

    fn collect_cards(&self, after: &str, limit: usize) -> Vec<CardPreview> {
        self.manifest
            .cards_by_id
            .range::<str, _>((Bound::Excluded(after), Bound::Unbounded))
            .take(limit + 1)
            .map(|(_id, entry)| self.card_preview_from_card_entry(entry))
            .collect()
    }

    fn card_preview_from_card_entry(&self, entry: &CardManifestEntry) -> CardPreview {
        let preview_url = entry
            .preview_relative_path
            .as_deref()
            .map(|path| self.preview_url_builder.build(path));

        CardPreview {
            card_id: entry.card_id.clone(),
            title: entry.title_slug.clone(),
            preview_url,
        }
    }
}

impl CardRepository for ManifestRepo {
    async fn find_card_path_by_id(
        &self,
        card_id: &str,
    ) -> Result<Option<PathBuf>, CardRepositoryError> {
        Ok(self.find_card_path(card_id))
    }

    async fn list_cards(&self, query: ListCardsQuery) -> Result<ListCardsRes, ListCardsError> {
        Ok(self.list_cards_page(query))
    }
}

#[inline]
fn select_asset(assets: &[AssetEntry]) -> Option<&AssetEntry> {
    assets
        .iter()
        .find(|asset| asset.variant == AssetVariant::Base)
        .or_else(|| assets.first())
}

#[cfg(test)]
mod tests {
    use crate::models::{AssetEntry, AssetVariant, CardManifestEntry, ListCardsQuery, Manifest};

    use super::{AssetPathBuilder, ManifestRepo, PreviewUrlBuilder};
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

        let page = repo.list_cards_page(query);

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

        let page = repo.list_cards_page(query);

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

        let page = repo.list_cards_page(ListCardsQuery {
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

        let path = repo.find_card_path("001");

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

        let path = repo.find_card_path("015");

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
    fn preview_url_builder_builds_public_url_with_manifest_suffix() {
        let builder = PreviewUrlBuilder::new("assets/previews/eoj/main_sets");

        let url = builder.build("set_1/001__flame-magus__variant-base__rev-02.jpeg");

        assert_eq!(
            url,
            "/previews/eoj/main_sets/set_1/001__flame-magus__variant-base__rev-02.jpeg"
        );
    }

    #[test]
    fn preview_url_builder_builds_public_url_for_root_without_suffix() {
        let builder = PreviewUrlBuilder::new("assets/previews");

        let url = builder.build("set_1/001.jpeg");

        assert_eq!(url, "/previews/set_1/001.jpeg");
    }

    #[test]
    fn asset_path_builder_builds_path_when_relative_path_already_contains_filename() {
        let builder = AssetPathBuilder::new("assets/images/eoj/main_sets");
        let asset = test_asset(AssetVariant::Base, "001__base.jpeg", "set_1/001__base.jpeg");

        let path = builder.build(&asset);

        assert_eq!(
            path,
            Path::new("assets/images/eoj/main_sets").join("set_1/001__base.jpeg")
        );
    }

    #[test]
    fn asset_path_builder_appends_filename_when_relative_path_is_directory() {
        let builder = AssetPathBuilder::new("assets/images/eoj/main_sets");
        let asset = test_asset(
            AssetVariant::Blank,
            "015__warden-hilda__variant-blank__rev-01.jpeg",
            "set_1/015",
        );

        let path = builder.build(&asset);

        assert_eq!(
            path,
            Path::new("assets/images/eoj/main_sets")
                .join("set_1/015")
                .join("015__warden-hilda__variant-blank__rev-01.jpeg")
        );
    }

    #[test]
    #[should_panic(
        expected = "invalid manifest.preview_root 'assets/private-previews/eoj/main_sets': expected path under 'assets/previews'"
    )]
    fn preview_url_builder_panics_for_non_public_root() {
        let _builder = PreviewUrlBuilder::new("assets/private-previews/eoj/main_sets");
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
