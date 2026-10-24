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

mod action;
mod api;
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
        // Fail startup if the worker cannot run instead of silently losing
        // power coordination after telling the supervisor it started.
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let (server, shared, requests) = api::Server::bind(&runtime, &socket)?;
        let worker = thread::Builder::new()
            .name("session-power".into())
            .spawn(move || {
                executor.block_on(async {
                    tokio::select! {
                        () = coordinator::run(runtime, socket, shared, requests) => {},
                        () = async {
                            if let Err(error) = server.run().await {
                                tracing::error!(%error, "power control endpoint stopped");
                            }
                            // Losing the optional shell endpoint must never
                            // release the established sleep delay or lid policy.
                            std::future::pending::<()>().await;
                        } => {},
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
