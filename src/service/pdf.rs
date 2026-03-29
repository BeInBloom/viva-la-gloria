use std::{io, path::PathBuf, sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

use crate::{
    contracts::CardRepository,
    errors::{PdfError, PdfInputError, PdfInternalError},
    http::dto::normalize_card_id,
};

use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use fpdf::{Fpdf, ImageOptions, Orientation, PageSize, Pdf, Unit, UnitVec2};
use tokio::{
    sync::{Notify, Semaphore},
    task::{self, JoinHandle},
};

const PDF_GENERATION_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_OUTPUT_DIR: &str = "./generated-pdf";
const PAGE_SIZE_MM: SizeMm = SizeMm::new(210.0, 297.0);
const CARD_SIZE_MM: SizeMm = SizeMm::new(63.0, 88.0);
const MAX_PARALLEL_JOBS: usize = 4;

const CLEANUP_PERIOD: Duration = Duration::from_secs(10);
const FILE_TTL: Duration = Duration::from_mins(10);
const MAX_DIR_SIZE: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
enum DeletionReason {
    Expired,
    Oversized,
}

impl DeletionReason {
    fn as_str(self) -> &'static str {
        match self {
            DeletionReason::Expired => "expired",
            DeletionReason::Oversized => "oversized",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeneratedPdfCleaner {
    ttl: Duration,
    max_dir_size_bytes: u64,
}

impl GeneratedPdfCleaner {
    pub fn new(ttl: Duration, max_dir_size_bytes: u64) -> Self {
        Self {
            ttl,
            max_dir_size_bytes,
        }
    }

    pub async fn cleanup_dir(&self, dir: &Path) -> io::Result<()> {
        let files = self.scan_files(dir).await?;
        let plan = self.build_cleanup_plan(files, SystemTime::now());
        self.apply_cleanup_plan(plan).await;
        Ok(())
    }

    async fn scan_files(&self, dir: &Path) -> io::Result<Vec<FileInfo>> {
        let mut entries = tokio::fs::read_dir(dir).await?;
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };

            if !file_type.is_file() {
                continue;
            }

            let path = entry.path();

            let Ok(metadata) = entry.metadata().await else {
                continue;
            };

            let Ok(modified) = metadata.modified() else {
                continue;
            };

            files.push(FileInfo {
                path,
                modified,
                size: metadata.len(),
            });
        }

        Ok(files)
    }

    fn build_cleanup_plan(&self, files: Vec<FileInfo>, now: SystemTime) -> CleanupPlan {
        let (expired, survivors) = self.split_expired_files(files, now);
        let oversized = self.select_oversized_files(survivors);

        CleanupPlan { expired, oversized }
    }

    fn split_expired_files(
        &self,
        files: Vec<FileInfo>,
        now: SystemTime,
    ) -> (Vec<PathBuf>, Vec<FileInfo>) {
        let mut expired = Vec::new();
        let mut survivors = Vec::new();

        for file in files {
            match now.duration_since(file.modified) {
                Ok(age) if age >= self.ttl => expired.push(file.path),
                Ok(_) | Err(_) => survivors.push(file),
            }
        }

        (expired, survivors)
    }

    fn select_oversized_files(&self, mut files: Vec<FileInfo>) -> Vec<PathBuf> {
        let total_size: u64 = files.iter().map(|file| file.size).sum();

        if total_size <= self.max_dir_size_bytes {
            return Vec::new();
        }

        files.sort_by_key(|file| file.modified);

        let mut current_size = total_size;
        let mut files_to_delete = Vec::new();

        for file in files {
            if current_size <= self.max_dir_size_bytes {
                break;
            }

            current_size = current_size.saturating_sub(file.size);
            files_to_delete.push(file.path);
        }

        files_to_delete
    }

    async fn apply_cleanup_plan(&self, plan: CleanupPlan) {
        self.delete_files(plan.expired, DeletionReason::Expired)
            .await;
        self.delete_files(plan.oversized, DeletionReason::Oversized)
            .await;
    }

    async fn delete_files(&self, paths: Vec<PathBuf>, reason: DeletionReason) {
        for path in paths {
            if let Err(err) = tokio::fs::remove_file(&path).await {
                eprintln!(
                    "failed to delete {} file {}: {err}",
                    reason.as_str(),
                    path.to_string_lossy(),
                );
            }
        }
    }
}

#[derive(Debug)]
struct FileInfo {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
}

#[derive(Debug, Default)]
struct CleanupPlan {
    expired: Vec<PathBuf>,
    oversized: Vec<PathBuf>,
}

#[derive(Debug)]
struct StorageCore {
    output_dir: PathBuf,
    cleaner: GeneratedPdfCleaner,
    cleanup_period: Duration,
}

#[derive(Debug)]
pub struct Stopped;

#[derive(Debug)]
pub struct Running {
    handle: JoinHandle<()>,
    cleanup_notify: Arc<Notify>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[derive(Debug)]
pub struct GeneratedPdfStorage<State> {
    core: StorageCore,
    state: State,
}

impl GeneratedPdfStorage<Stopped> {
    pub fn new(
        path: impl Into<PathBuf>,
        cleaner: GeneratedPdfCleaner,
        cleanup_period: Duration,
    ) -> Self {
        let output_dir = path.into();
        fs::create_dir_all(&output_dir).unwrap_or_else(|err| {
            panic!("cant create dir {}: {}", output_dir.to_string_lossy(), err)
        });

        Self {
            core: StorageCore {
                output_dir,
                cleaner,
                cleanup_period,
            },
            state: Stopped,
        }
    }

    pub fn start(self) -> GeneratedPdfStorage<Running> {
        let cleanup_notify = Arc::new(Notify::new());
        let handle = spawn_cleanup_task(
            self.core.output_dir.clone(),
            self.core.cleaner.clone(),
            self.core.cleanup_period,
            Arc::clone(&cleanup_notify),
        );

        GeneratedPdfStorage {
            core: self.core,
            state: Running {
                handle,
                cleanup_notify,
            },
        }
    }
}

impl GeneratedPdfStorage<Running> {
    pub fn next_output_path(&self) -> PathBuf {
        self.state.cleanup_notify.notify_one();

        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();

        self.core
            .output_dir
            .join(format!("cards-{timestamp_ms}.pdf"))
    }

    pub fn output_dir(&self) -> PathBuf {
        self.core.output_dir.clone()
    }
}

fn spawn_cleanup_task(
    output_dir: PathBuf,
    cleaner: GeneratedPdfCleaner,
    cleanup_period: Duration,
    cleanup_notify: Arc<Notify>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(cleanup_period);

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = cleanup_notify.notified() => {}
            }

            if let Err(err) = cleaner.cleanup_dir(&output_dir).await {
                eprintln!("cleanup failed for {}: {err}", output_dir.to_string_lossy());
            }
        }
    })
}

pub struct PdfService<R> {
    card_repository: Arc<R>,
    pdf_storage: GeneratedPdfStorage<Running>,
    layout: Layout,
    blocking_slots: Arc<Semaphore>,
}

impl<R> PdfService<R>
where
    R: CardRepository,
{
    pub fn new(card_repository: Arc<R>) -> Self {
        let pdf_storage = GeneratedPdfStorage::new(
            DEFAULT_OUTPUT_DIR,
            GeneratedPdfCleaner::new(FILE_TTL, MAX_DIR_SIZE),
            CLEANUP_PERIOD,
        )
        .start();

        Self {
            card_repository,
            pdf_storage,
            layout: Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM),
            blocking_slots: Arc::new(Semaphore::new(MAX_PARALLEL_JOBS)),
        }
    }

    pub async fn generate(&self, requested_card_ids: Vec<String>) -> Result<PathBuf, PdfError> {
        let card_ids = requested_card_ids
            .into_iter()
            .map(normalize_card_id)
            .collect::<Vec<_>>();

        ensure_cards_were_requested(&card_ids)?;

        let card_paths = self.find_card_paths(&card_ids).await?;
        let cancellation_token = CancellationToken::new();
        let _cancel_generation_on_drop = cancellation_token.clone().drop_guard();

        let handle = tokio::time::timeout(
            PDF_GENERATION_TIMEOUT,
            self.render_pdf(&card_paths, cancellation_token.clone()),
        );

        match handle.await {
            Ok(result) => result,
            Err(_) => {
                cancellation_token.cancel();
                Err(PdfInternalError::PdfGenerationTimedOut.into())
            }
        }
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

    async fn render_pdf(
        &self,
        card_paths: &[PathBuf],
        cancellation_token: CancellationToken,
    ) -> Result<PathBuf, PdfError> {
        let permit = Arc::clone(&self.blocking_slots)
            .try_acquire_owned()
            .map_err(|_| PdfInputError::PdfGenerationBusy)?;

        let output_dir = self.pdf_storage.output_dir();

        let output_path = self.pdf_storage.next_output_path();

        let layout = self.layout;
        let card_paths = card_paths.to_vec();

        let handle = task::spawn_blocking(move || -> Result<PathBuf, PdfError> {
            let _permit = permit;

            fs::create_dir_all(&output_dir).map_err(|source| {
                PdfInternalError::CreateOutputDir {
                    path: output_dir,
                    source,
                }
            })?;

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
}

#[inline]
fn ensure_cards_were_requested(card_ids: &[String]) -> Result<(), PdfError> {
    if card_ids.is_empty() {
        return Err(PdfInputError::EmptyCardIds.into());
    }

    Ok(())
}

#[inline]
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

    for page in card_paths.chunks(layout.cards_per_page) {
        ensure_generation_not_cancelled(cancellation_token)?;
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

    Ok(())
}

#[inline]
fn ensure_generation_not_cancelled(cancellation_token: &CancellationToken) -> Result<(), PdfError> {
    if cancellation_token.is_cancelled() {
        return Err(PdfInputError::PdfGenerationCancelled.into());
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{
        CARD_SIZE_MM, CLEANUP_PERIOD, FILE_TTL, GeneratedPdfCleaner, GeneratedPdfStorage, Layout,
        MAX_DIR_SIZE, PAGE_SIZE_MM, PdfService, ensure_cards_were_requested,
    };
    use crate::{
        contracts::CardRepository,
        errors::{CardRepositoryError, ListCardsError, PdfError, PdfInputError},
        models::{ListCardsQuery, ListCardsRes},
    };
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::Semaphore;

    #[test]
    fn ensure_cards_were_requested_rejects_empty_input() {
        let error = ensure_cards_were_requested(&[]).unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::EmptyCardIds)
        ));
    }

    #[test]
    fn layout_for_a4_cards_uses_expected_grid_and_margins() {
        let layout = Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM);

        assert_eq!(layout.cards_per_row, 3);
        assert_eq!(layout.cards_per_page, 9);
        assert_eq!(layout.margin_left_mm, 10.5);
        assert_eq!(layout.margin_top_mm, 16.5);
    }

    #[test]
    fn layout_positions_center_card_grid_slots() {
        let layout = Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM);

        assert_eq!(layout.position_for_slot(0), (10.5, 16.5));
        assert_eq!(layout.position_for_slot(4), (73.5, 104.5));
        assert_eq!(layout.position_for_slot(8), (136.5, 192.5));
    }

    #[tokio::test]
    async fn find_card_paths_returns_missing_ids_in_request_order() {
        let generator = test_service(
            StubRepo::new([
                ("001", Some("tests/001.jpeg")),
                ("002", None),
                ("003", Some("tests/003.jpeg")),
                ("004", None),
            ]),
            1,
            unique_test_path("missing-paths"),
        );

        let error = generator
            .find_card_paths(&["002".to_owned(), "001".to_owned(), "004".to_owned()])
            .await
            .unwrap_err();

        match error {
            PdfError::BadRequest(PdfInputError::CardsNotFound { card_ids }) => {
                assert_eq!(card_ids, vec!["002".to_owned(), "004".to_owned()]);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn generate_returns_busy_when_no_blocking_slots_are_available() {
        let generator = test_service(
            StubRepo::new([("001", Some("tests/001.jpeg"))]),
            0,
            unique_test_path("busy"),
        );

        let error = generator
            .generate(vec!["001".to_owned()])
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::PdfGenerationBusy)
        ));
    }

    #[tokio::test]
    async fn generate_returns_cancelled_when_token_is_already_cancelled() {
        let output_dir = unique_test_path("cancelled");
        let generator = test_service(
            StubRepo::new([("001", Some("tests/001.jpeg"))]),
            1,
            output_dir.clone(),
        );

        let error = generator
            .render_pdf(&[PathBuf::from("tests/001.jpeg")], {
                let token = tokio_util::sync::CancellationToken::new();
                token.cancel();
                token
            })
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            PdfError::BadRequest(PdfInputError::PdfGenerationCancelled)
        ));

        let _ = std::fs::remove_dir_all(output_dir);
    }

    fn test_service(repo: StubRepo, permits: usize, output_dir: PathBuf) -> PdfService<StubRepo> {
        PdfService {
            card_repository: Arc::new(repo),
            pdf_storage: GeneratedPdfStorage::new(
                output_dir,
                GeneratedPdfCleaner::new(FILE_TTL, MAX_DIR_SIZE),
                CLEANUP_PERIOD,
            )
            .start(),
            layout: Layout::new(PAGE_SIZE_MM, CARD_SIZE_MM),
            blocking_slots: Arc::new(Semaphore::new(permits)),
        }
    }

    fn unique_test_path(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("eoj-card-generator-{name}-{timestamp}"))
    }

    struct StubRepo {
        card_paths: BTreeMap<String, Option<PathBuf>>,
    }

    impl StubRepo {
        fn new<const N: usize>(card_paths: [(&str, Option<&str>); N]) -> Self {
            Self {
                card_paths: card_paths
                    .into_iter()
                    .map(|(card_id, path)| (card_id.to_owned(), path.map(PathBuf::from)))
                    .collect(),
            }
        }
    }

    impl CardRepository for StubRepo {
        async fn find_card_path_by_id(
            &self,
            card_id: &str,
        ) -> Result<Option<PathBuf>, CardRepositoryError> {
            Ok(self.card_paths.get(card_id).cloned().flatten())
        }

        async fn list_cards(&self, _query: ListCardsQuery) -> Result<ListCardsRes, ListCardsError> {
            Ok(ListCardsRes {
                items: Vec::new(),
                next_cursor: None,
            })
        }
    }
}
