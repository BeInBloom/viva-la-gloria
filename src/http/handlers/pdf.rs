use axum::{Json, extract::State};

use crate::{
    errors::{PdfError, PdfInternalError},
    http::{
        dto::{GeneratePdfRequest, GeneratePdfResponse},
        state::AppState,
    },
};

pub(crate) async fn generate_pdf(
    State(state): State<AppState>,
    Json(payload): Json<GeneratePdfRequest>,
) -> Result<Json<GeneratePdfResponse>, PdfError> {
    let path = state.pdf_service.generate(payload.card_ids).await?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(PdfInternalError::GeneratedPdfFileNameMissing)?;

    Ok(Json(GeneratePdfResponse {
        path: format!("/generated-pdf/{file_name}"),
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        net::IpAddr,
        path::{Path, PathBuf},
        sync::{Arc, OnceLock},
        time::Duration,
    };

    use axum::{Json, extract::State};
    use moka::future::Cache;
    use tokio::sync::Mutex;

    use crate::{
        errors::{PdfError, PdfInputError},
        models::{AssetEntry, AssetVariant, CardManifestEntry, Manifest},
        repo::cards::ManifestRepo,
        service::pdf::PdfService,
    };

    use super::generate_pdf;
    use crate::http::{dto::GeneratePdfRequest, state::AppState};

    #[tokio::test]
    async fn generate_pdf_returns_public_path_for_generated_file() {
        let _guard = pdf_generation_lock().lock().await;
        let state = test_state(manifest_with_single_card());

        let Json(response) = generate_pdf(
            State(state),
            Json(GeneratePdfRequest {
                card_ids: vec!["1".to_owned()],
            }),
        )
        .await
        .expect("pdf generation should succeed");

        assert!(response.path.starts_with("/generated-pdf/cards-"));
        assert!(response.path.ends_with(".pdf"));

        let generated_file =
            GeneratedFile::new(PathBuf::from(response.path.trim_start_matches('/')));
        assert!(
            generated_file.path().exists(),
            "generated file should exist on disk"
        );
        assert_eq!(
            generated_file.path().parent(),
            Some(Path::new("generated-pdf"))
        );
        assert!(
            fs::metadata(generated_file.path())
                .expect("generated pdf metadata")
                .len()
                > 0,
            "generated pdf should not be empty"
        );
    }

    #[tokio::test]
    async fn generate_pdf_rejects_empty_card_id_lists() {
        let state = test_state(manifest_with_single_card());

        let error = generate_pdf(
            State(state),
            Json(GeneratePdfRequest {
                card_ids: Vec::new(),
            }),
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::EmptyCardIds)
        ));
    }

    fn pdf_generation_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn test_state(manifest: Manifest) -> AppState {
        let repo = Arc::new(ManifestRepo::new(manifest).expect("repo should be created"));

        AppState {
            pdf_service: Arc::new(PdfService::new(Arc::clone(&repo)).expect("service should be created")),
            card_repository: repo,
            pdf_rate_limit: test_rate_limit_cache(),
        }
    }

    fn test_rate_limit_cache() -> Cache<IpAddr, ()> {
        Cache::builder()
            .time_to_live(Duration::from_secs(10))
            .max_capacity(128)
            .build()
    }

    fn manifest_with_single_card() -> Manifest {
        Manifest {
            asset_root: "assets/images/eoj/main_sets".to_owned(),
            preview_root: "assets/previews/eoj/main_sets".to_owned(),
            cards_by_id: BTreeMap::from([(
                "001".to_owned(),
                CardManifestEntry {
                    set_name: "set_1".to_owned(),
                    card_id: "001".to_owned(),
                    title_slug: "flame-magus".to_owned(),
                    preview_relative_path: Some(
                        "set_1/001__flame-magus__variant-base__rev-02.jpeg".to_owned(),
                    ),
                    review_flags: Vec::new(),
                    assets: vec![AssetEntry {
                        variant: AssetVariant::Base,
                        variant_index: None,
                        asset_revision: Some("02".to_owned()),
                        processing_profile: "topaz-denoise".to_owned(),
                        faceai: false,
                        filename: "001__flame-magus__variant-base__rev-02.jpeg".to_owned(),
                        relative_path: "set_1/001__flame-magus__variant-base__rev-02.jpeg"
                            .to_owned(),
                    }],
                },
            )]),
        }
    }

    struct GeneratedFile {
        path: PathBuf,
    }

    impl GeneratedFile {
        fn new(path: PathBuf) -> Self {
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for GeneratedFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
