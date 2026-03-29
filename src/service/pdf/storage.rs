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
