use std::time::Duration;
use tokio::sync::{mpsc, watch};

use super::{TrayCommand, TraySnapshot, session::Session};

/// The UI's bounded, nonblocking command endpoint.
#[derive(Clone, Debug)]
pub struct TrayHandle {
    sender: mpsc::Sender<TrayCommand>,
}

impl TrayHandle {
    pub fn is_available(&self) -> bool {
        !self.sender.is_closed()
    }

    pub fn try_send(&self, command: TrayCommand) -> Result<(), String> {
        self.sender.try_send(command).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => "Tray application is busy; try again".into(),
            mpsc::error::TrySendError::Closed(_) => "Tray service is unavailable".into(),
        })
    }
}

/// Create channels without starting any task or connecting to any bus.
/// `None` intentionally disables the tray outside a private Hearthspace session.
pub fn channel(address: Option<String>) -> (TrayHandle, watch::Receiver<TraySnapshot>, TrayWorker) {
    let (sender, commands) = mpsc::channel(32);
    let (updates, receiver) = watch::channel(TraySnapshot::default());
    (
        TrayHandle { sender },
        receiver,
        TrayWorker {
            address,
            commands,
            updates,
        },
    )
}

pub struct TrayWorker {
    address: Option<String>,
    commands: mpsc::Receiver<TrayCommand>,
    updates: watch::Sender<TraySnapshot>,
}

impl TrayWorker {
    /// Own all connection and request futures inside the UI task's lifetime.
    /// Cancellation drops bounded JoinSets and the private D-Bus connection.
    pub async fn run(mut self) {
        let Some(address) = self.address.take().filter(|value| !value.is_empty()) else {
            return;
        };
        let mut generation = 0;
        loop {
            if self.commands.is_closed() || self.updates.is_closed() {
                break;
            }
            let result = match tokio::time::timeout(
                Duration::from_secs(3),
                Session::connect(&address),
            )
            .await
            {
                Ok(Ok(mut session)) => {
                    let result = session
                        .run(&mut self.commands, &self.updates, &mut generation)
                        .await;
                    session.shutdown().await;
                    result
                }
                Ok(Err(error)) => Err(error),
                Err(_) => Err("Timed out connecting to the tray service".into()),
            };
            if let Err(error) = result {
                tracing::warn!(%error, "tray connection unavailable");
                self.updates.send_replace(TraySnapshot {
                    items: Vec::new(),
                    error: Some(error),
                });
            } else {
                break;
            }
            // Ignore queued stale commands while disconnected, then retry.
            let delay = tokio::time::sleep(Duration::from_secs(1));
            tokio::pin!(delay);
            loop {
                tokio::select! {
                    _ = &mut delay => break,
                    _ = self.updates.closed() => return,
                    command = self.commands.recv() => if command.is_none() { return; },
                }
            }
        }
    }
}
