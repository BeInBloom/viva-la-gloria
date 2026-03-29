use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::{
    sync::Notify,
    task::{self, JoinHandle},
};

#[derive(Debug, Clone, Copy)]
enum DeletionReason {
    Expired,
    Oversized,
}

impl DeletionReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Expired => "expired",
            Self::Oversized => "oversized",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct GeneratedPdfCleaner {
    ttl: Duration,
    max_dir_size_bytes: u64,
}

impl GeneratedPdfCleaner {
    pub(super) fn new(ttl: Duration, max_dir_size_bytes: u64) -> Self {
        Self {
            ttl,
            max_dir_size_bytes,
        }
    }

    pub(super) async fn cleanup_dir(&self, dir: &Path) -> io::Result<()> {
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
pub(super) struct Stopped;

#[derive(Debug)]
pub(super) struct Running {
    handle: JoinHandle<()>,
    cleanup_notify: Arc<Notify>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[derive(Debug)]
pub(super) struct GeneratedPdfStorage<State> {
    core: StorageCore,
    state: State,
}

impl GeneratedPdfStorage<Stopped> {
    pub(super) fn new(
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

    pub(super) fn start(self) -> GeneratedPdfStorage<Running> {
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
    pub(super) fn next_output_path(&self) -> PathBuf {
        self.state.cleanup_notify.notify_one();

        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();

        self.core
            .output_dir
            .join(format!("cards-{timestamp_ms}.pdf"))
    }

    #[allow(dead_code)]
    pub(super) fn output_dir(&self) -> PathBuf {
        self.core.output_dir.clone()
    }
}

fn spawn_cleanup_task(
    output_dir: PathBuf,
    cleaner: GeneratedPdfCleaner,
    cleanup_period: Duration,
    cleanup_notify: Arc<Notify>,
) -> JoinHandle<()> {
    task::spawn(async move {
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

#[cfg(test)]
mod tests {
    use super::{CleanupPlan, FileInfo, GeneratedPdfCleaner, GeneratedPdfStorage};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn split_expired_files_expires_ttl_boundary_and_keeps_future_files() {
        let cleaner = GeneratedPdfCleaner::new(Duration::from_secs(10), u64::MAX);
        let now = UNIX_EPOCH + Duration::from_secs(20);
        let boundary = PathBuf::from("boundary.pdf");
        let recent = PathBuf::from("recent.pdf");
        let future = PathBuf::from("future.pdf");

        let (expired, survivors) = cleaner.split_expired_files(
            vec![
                file_info(boundary.clone(), now - Duration::from_secs(10), 10),
                file_info(recent.clone(), now - Duration::from_secs(9), 10),
                file_info(future.clone(), now + Duration::from_secs(1), 10),
            ],
            now,
        );

        assert_eq!(expired, vec![boundary]);
        assert_eq!(file_paths(&survivors), vec![recent, future]);
    }

    #[test]
    fn select_oversized_files_deletes_oldest_files_first() {
        let cleaner = GeneratedPdfCleaner::new(Duration::from_secs(60), 50);
        let base = UNIX_EPOCH + Duration::from_secs(100);
        let oldest = PathBuf::from("oldest.pdf");
        let middle = PathBuf::from("middle.pdf");
        let newest = PathBuf::from("newest.pdf");

        let to_delete = cleaner.select_oversized_files(vec![
            file_info(newest.clone(), base + Duration::from_secs(3), 20),
            file_info(oldest.clone(), base + Duration::from_secs(1), 40),
            file_info(middle.clone(), base + Duration::from_secs(2), 10),
        ]);

        assert_eq!(to_delete, vec![oldest]);
    }

    #[test]
    fn build_cleanup_plan_excludes_expired_files_from_oversize_calculation() {
        let cleaner = GeneratedPdfCleaner::new(Duration::from_secs(10), 100);
        let now = UNIX_EPOCH + Duration::from_secs(30);
        let expired = PathBuf::from("expired.pdf");
        let oversized = PathBuf::from("oversized.pdf");
        let newest = PathBuf::from("newest.pdf");

        let plan = cleaner.build_cleanup_plan(
            vec![
                file_info(expired.clone(), now - Duration::from_secs(11), 100),
                file_info(oversized.clone(), now - Duration::from_secs(5), 60),
                file_info(newest.clone(), now - Duration::from_secs(4), 60),
            ],
            now,
        );

        assert_eq!(
            plan_paths(plan),
            (
                vec![expired],
                vec![oversized],
            )
        );
    }

    #[tokio::test]
    async fn cleanup_dir_removes_root_files_but_keeps_nested_directories() {
        let dir = TestDir::new("cleanup-root-files");
        let root_file = write_file(dir.path(), "root.pdf", 8);
        let nested_dir = dir.path().join("nested");
        fs::create_dir_all(&nested_dir).expect("create nested dir");
        let nested_file = write_file(&nested_dir, "nested.pdf", 8);
        let cleaner = GeneratedPdfCleaner::new(Duration::ZERO, u64::MAX);

        cleaner.cleanup_dir(dir.path()).await.expect("cleanup succeeds");

        assert!(!root_file.exists(), "expected root file to be deleted");
        assert!(nested_dir.exists(), "expected nested directory to remain");
        assert!(nested_file.exists(), "expected nested file to remain untouched");
    }

    #[tokio::test]
    async fn next_output_path_notifies_cleanup_and_returns_pdf_path() {
        let root = TestDir::new("storage");
        let output_dir = root.path().join("generated");
        let storage = GeneratedPdfStorage::new(
            &output_dir,
            GeneratedPdfCleaner::new(Duration::ZERO, u64::MAX),
            Duration::from_secs(60),
        )
        .start();

        assert!(output_dir.exists(), "storage should create the output directory");

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stale_file = write_file(&output_dir, "stale.pdf", 8);
        let output_path = storage.next_output_path();

        assert_eq!(output_path.parent(), Some(output_dir.as_path()));

        let file_name = output_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("generated path has utf-8 file name");
        assert!(file_name.starts_with("cards-"));
        assert!(file_name.ends_with(".pdf"));

        wait_until_missing(&stale_file).await;
    }

    fn file_info(path: PathBuf, modified: SystemTime, size: u64) -> FileInfo {
        FileInfo {
            path,
            modified,
            size,
        }
    }

    fn file_paths(files: &[FileInfo]) -> Vec<PathBuf> {
        files.iter().map(|file| file.path.clone()).collect()
    }

    fn plan_paths(plan: CleanupPlan) -> (Vec<PathBuf>, Vec<PathBuf>) {
        (plan.expired, plan.oversized)
    }

    fn write_file(dir: &Path, file_name: &str, size: usize) -> PathBuf {
        fs::create_dir_all(dir).expect("create test dir");
        let path = dir.join(file_name);
        fs::write(&path, vec![b'x'; size]).expect("write test file");
        path
    }

    async fn wait_until_missing(path: &Path) {
        for _ in 0..50 {
            if !path.exists() {
                return;
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        panic!("expected {} to be deleted", path.to_string_lossy());
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("eoj-card-generator-{name}-{suffix}"));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
