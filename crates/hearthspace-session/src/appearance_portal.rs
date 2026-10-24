//! Session-owned appearance publication. No desktop preference store is written.
use std::{
    io,
    path::PathBuf,
    sync::{atomic::AtomicBool, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tokio::sync::oneshot;

#[path = "appearance_portal/fallback.rs"]
mod fallback;
#[path = "appearance_portal/settings.rs"]
mod settings;
#[cfg(test)]
#[path = "appearance_portal/tests.rs"]
mod tests;

pub(crate) use settings::NAME;
use settings::Settings;

pub(crate) struct AppearancePortal {
    bus: String,
    socket: PathBuf,
    worker: Option<Worker>,
    restarts_left: usize,
}

struct Worker {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<io::Result<()>>>,
}

impl AppearancePortal {
    pub(crate) fn start(
        bus: String,
        socket: PathBuf,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> io::Result<Self> {
        let worker = Worker::start(bus.clone(), socket.clone(), deadline, cancelled, false)?;
        Ok(Self {
            bus,
            socket,
            worker: Some(worker),
            restarts_left: 2,
        })
    }

    pub(crate) fn poll(&mut self, cancelled: &AtomicBool) {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.thread.as_ref().unwrap().is_finished())
        {
            return;
        }
        self.worker.take();
        if self.restarts_left == 0 {
            return;
        }
        self.restarts_left -= 1;
        tracing::warn!("restarting the private appearance portal");
        match Worker::start(
            self.bus.clone(),
            self.socket.clone(),
            Instant::now() + Duration::from_secs(3),
            cancelled,
            true,
        ) {
            Ok(worker) => self.worker = Some(worker),
            Err(error) => tracing::warn!(%error, "appearance portal restart failed"),
        }
    }
}

impl Worker {
    fn start(
        bus: String,
        socket: PathBuf,
        deadline: Instant,
        cancelled: &AtomicBool,
        reannounce: bool,
    ) -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (stop, stopped) = oneshot::channel();
        let (ready, readiness) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("appearance-portal".into())
            .spawn(move || {
                runtime.block_on(async {
                    let result = serve(&bus, &socket, ready.clone(), reannounce, stopped).await;
                    if let Err(error) = &result {
                        let _ = ready.try_send(Err(error.to_string()));
                        tracing::error!(%error, "appearance portal stopped");
                    }
                    result
                })
            })?;
        let worker = Self {
            stop: Some(stop),
            thread: Some(thread),
        };
        loop {
            let remaining = crate::readiness::remaining(deadline, cancelled)?;
            match readiness.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok(Ok(())) => return Ok(worker),
                Ok(Err(error)) => return Err(io::Error::other(error)),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("appearance portal stopped during startup"));
                }
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn serve(
    bus: &str,
    socket: &std::path::Path,
    ready: mpsc::SyncSender<Result<(), String>>,
    reannounce: bool,
    mut stopped: oneshot::Receiver<()>,
) -> io::Result<()> {
    // This bounded IPC runs only on this dedicated worker, never on a UI or
    // supervisor thread. A temporarily unavailable daemon retains the last
    // published value rather than broadcasting a spurious reset.
    use hearthspace_ipc::settings_snapshot::{client_get_snapshot_at, client_wait_for_change_at};
    let setup = async {
        let path = socket.to_path_buf();
        let initial = tokio::task::spawn_blocking(move || client_get_snapshot_at(&path))
            .await
            .map_err(io::Error::other)??;
        let cursor = Some(initial.cursor());
        let initial = initial.settings.appearance;
        let fallback =
            tokio::time::timeout(Duration::from_secs(2), fallback::Fallback::connect(bus))
                .await
                .map_err(io::Error::other)?
                .map_err(io::Error::other)?;
        let shared = Settings::with_fallback(initial, fallback.clone());
        let connection = tokio::time::timeout(
            Duration::from_secs(2),
            settings::connect(bus, shared.clone()),
        )
        .await
        .map_err(io::Error::other)?
        .map_err(io::Error::other)?;
        Ok::<_, io::Error>((cursor, fallback, shared, connection))
    };
    let (mut cursor, fallback, shared, connection) = tokio::select! {
        _ = &mut stopped => return Ok(()),
        result = setup => result?,
    };
    if reannounce {
        // Existing clients retain their cached settings when a backend owner
        // disappears. Recover changes made while this worker was unavailable.
        shared
            .announce(&connection)
            .await
            .map_err(io::Error::other)?;
    }
    ready
        .send(Ok(()))
        .map_err(|_| io::Error::other("appearance startup was cancelled"))?;
    let result = {
        let forwarded = fallback.forward(&connection);
        tokio::pin!(forwarded);
        loop {
            tokio::select! {
                _ = &mut stopped => break Ok(()),
                _ = connection.closed() => break Err(io::Error::other("private appearance bus closed")),
                result = &mut forwarded => break result.map_err(io::Error::other),
                result = async {
                    let path = socket.to_path_buf();
                    let after = cursor.clone();
                    tokio::task::spawn_blocking(move || match after {
                        Some(after) => client_wait_for_change_at(&path, &after, Duration::from_millis(500)),
                        None => client_get_snapshot_at(&path).map(Some),
                    }).await
                } => {
                    match result {
                        Ok(Ok(Some(snapshot))) => {
                            cursor = Some(snapshot.cursor());
                            if let Err(error) = shared.publish(&connection, snapshot.settings.appearance).await {
                                break Err(io::Error::other(error));
                            }
                        },
                        Ok(Ok(None)) => {},
                        _ => {
                            cursor = None;
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        },
                    }
                }
            }
        }
    };
    // Explicitly retire the name before the worker joins. Dropping a zbus
    // connection schedules asynchronous teardown and is not an ownership barrier.
    let _ = tokio::time::timeout(Duration::from_millis(500), connection.release_name(NAME)).await;
    let _ = connection.close().await;
    result
}
