use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use hf_hub::api::tokio::{ApiBuilder, Progress};
use sha2::{Digest, Sha256};

use crate::{
    MODEL_FILE, MODEL_REPOSITORY,
    model::{InstallPhase, InstallProgress},
    model_files,
};

type Callback = Arc<dyn Fn(InstallProgress) + Send + Sync>;

#[derive(Clone)]
struct DownloadProgress {
    callback: Callback,
    completed: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    last_emit_ms: Arc<AtomicU64>,
    started: Instant,
}

impl Progress for DownloadProgress {
    async fn init(&mut self, size: usize, _: &str) {
        self.total.store(size as u64, Ordering::Relaxed);
        self.emit();
    }
    async fn update(&mut self, size: usize) {
        self.completed.fetch_add(size as u64, Ordering::Relaxed);
        let now = self.started.elapsed().as_millis() as u64;
        let previous = self.last_emit_ms.load(Ordering::Relaxed);
        if now.saturating_sub(previous) >= 100
            && self
                .last_emit_ms
                .compare_exchange(previous, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            self.emit();
        }
    }
    async fn finish(&mut self) {
        self.completed
            .store(self.total.load(Ordering::Relaxed), Ordering::Relaxed);
        self.emit();
    }
}

impl DownloadProgress {
    fn emit(&self) {
        (self.callback)(InstallProgress {
            phase: InstallPhase::Downloading,
            completed: self.completed.load(Ordering::Relaxed),
            total: Some(self.total.load(Ordering::Relaxed)),
        });
    }
}

pub(crate) fn fetch(directory: &Path, callback: Callback) -> Result<PathBuf> {
    if let Some(path) = model_files::cached(directory) {
        return Ok(path);
    }
    let progress = DownloadProgress {
        callback,
        completed: Arc::default(),
        total: Arc::default(),
        last_emit_ms: Arc::default(),
        started: Instant::now(),
    };
    // This runtime is owned by the caller's dedicated worker, never the UI.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let api = ApiBuilder::new()
            .with_cache_dir(directory.join("hub"))
            .with_token(None)
            .with_progress(false)
            .with_max_files(2)
            .build()?;
        tokio::time::timeout(
            Duration::from_secs(7200),
            api.model(MODEL_REPOSITORY.into())
                .download_with_progress(MODEL_FILE, progress),
        )
        .await
        .context("D1 download exceeded two hours; retry to resume")?
        .context("Could not download the D1 Q4 model from Hugging Face")
    });
    runtime.shutdown_timeout(Duration::from_secs(1));
    result
}

/// Stream the digest: a 1.67 GB model must never be buffered in memory.
pub(crate) fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}
