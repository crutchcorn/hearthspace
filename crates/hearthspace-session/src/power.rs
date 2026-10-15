//! Native-session power coordination lives off the supervisor/compositor loops.
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

mod control;
mod coordinator;
mod logind;
mod policy;
mod status;

pub(crate) struct PowerManager {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PowerManager {
    pub(crate) fn start(runtime: PathBuf, socket: PathBuf) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let worker = thread::Builder::new()
            .name("session-power".into())
            .spawn(move || {
                let runtime_executor = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(executor) = runtime_executor else {
                    tracing::error!("could not start power coordinator runtime");
                    return;
                };
                executor.block_on(async {
                    tokio::select! {
                        () = coordinator::run(runtime, socket) => {},
                        () = async {
                            while !cancelled.load(Ordering::SeqCst) {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                        } => {},
                    }
                });
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for PowerManager {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
