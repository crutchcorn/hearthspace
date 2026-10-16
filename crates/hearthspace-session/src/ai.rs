//! Optional model warming never participates in the desktop startup deadline.

use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::Duration,
};

use hearthspace_ai::ModelManager;

pub(crate) struct SessionModel {
    stop: Sender<()>,
}

impl SessionModel {
    pub(crate) fn start() -> Option<Self> {
        let (stop, receiver) = mpsc::channel();
        match thread::Builder::new()
            .name("session-ai".into())
            .spawn(move || {
                let manager = match ModelManager::from_env() {
                    Ok(manager) => manager,
                    Err(error) => {
                        tracing::warn!(%error, "optional D1 model settings unavailable");
                        return;
                    }
                };
                maintain(
                    receiver,
                    Duration::from_secs(120),
                    || manager.warm_for_session(),
                    || manager.unload(),
                );
            }) {
            Ok(_) => Some(Self { stop }),
            Err(error) => {
                tracing::warn!(%error, "could not start optional D1 model worker");
                None
            }
        }
    }
}

impl Drop for SessionModel {
    fn drop(&mut self) {
        // Requests are bounded, but loading on CPU can still be slow. The finite
        // warm lease also expires after an abrupt exit or a load during logout.
        // Joining here would put optional inference on the shutdown path.
        let _ = self.stop.send(());
    }
}

fn maintain(
    stop: Receiver<()>,
    interval: Duration,
    mut warm: impl FnMut() -> hearthspace_ai::Result<bool>,
    unload: impl FnOnce() -> hearthspace_ai::Result<()>,
) {
    let mut warmed = false;
    loop {
        match warm() {
            Ok(loaded) => {
                if loaded && !warmed {
                    tracing::info!("D1 decision model is ready for this session");
                }
                warmed |= loaded;
            }
            Err(error) => tracing::warn!(%error, "optional D1 model could not be warmed"),
        }
        match stop.recv_timeout(interval) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    if warmed && let Err(error) = unload() {
        tracing::warn!(%error, "D1 will unload when its warm lease expires");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn installed_model_is_renewed_and_released_when_session_stops() {
        let (sender, receiver) = mpsc::channel();
        let loads = Cell::new(0);
        let released = Cell::new(false);
        maintain(
            receiver,
            Duration::ZERO,
            || {
                loads.set(loads.get() + 1);
                if loads.get() == 2 {
                    sender.send(()).unwrap();
                }
                Ok(true)
            },
            || {
                released.set(true);
                Ok(())
            },
        );
        assert_eq!(loads.get(), 2);
        assert!(released.get());
    }

    #[test]
    fn missing_model_is_not_downloaded_or_unloaded() {
        let (sender, receiver) = mpsc::channel();
        sender.send(()).unwrap();
        maintain(
            receiver,
            Duration::ZERO,
            || Ok(false),
            || panic!("an absent optional model must not be unloaded"),
        );
    }

    #[test]
    fn unavailable_runtime_is_nonfatal_and_session_can_still_stop() {
        let (sender, receiver) = mpsc::channel();
        sender.send(()).unwrap();
        maintain(
            receiver,
            Duration::ZERO,
            || Err(std::io::Error::other("Ollama unavailable").into()),
            || panic!("an unavailable model was never loaded"),
        );
    }
}
