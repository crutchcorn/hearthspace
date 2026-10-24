//! Primary identity has a separate lifetime from wl_output mode/hotplug events.
use super::{Snapshot, live_outputs};
use hearthspace_wayland_client::OutputInfo;
use std::{io, path::PathBuf, time::Duration};
use xilem::tokio::{self, sync::watch, time::Instant};

const RETRY: Duration = Duration::from_millis(500);
const REFRESH: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Primary {
    known: Option<String>,
}

impl Primary {
    fn update(&mut self, result: io::Result<Option<String>>, outputs: &[OutputInfo]) -> bool {
        let available = result.is_ok();
        // Valid None clears the authoritative primary. A transport failure keeps
        // known identity only while that connector is still actually connected.
        if let Ok(primary) = result {
            self.known = primary;
        }
        if self
            .known
            .as_ref()
            .is_some_and(|name| !outputs.iter().any(|output| output.connector() == *name))
        {
            self.known = None;
        }
        available
    }
}

pub(super) async fn deliver(
    mut receiver: watch::Receiver<Option<Vec<OutputInfo>>>,
    socket: Result<PathBuf, String>,
    mut publish: impl FnMut(Snapshot) -> bool,
) {
    let mut primary = Primary::default();
    let mut previous = None;
    let mut next = Instant::now() + RETRY;
    loop {
        tokio::select! {
            changed = receiver.changed() => { if changed.is_err() { return; } },
            _ = tokio::time::sleep_until(next) => {},
        }
        next = Instant::now() + RETRY;
        let Some(outputs) = receiver.borrow_and_update().clone() else {
            continue;
        };
        let socket = socket.clone();
        let result = tokio::task::spawn_blocking(move || match socket {
            Ok(socket) => hearthspace_ipc::outputs::client_get_primary_output_at(&socket),
            Err(error) => Err(io::Error::other(error)),
        })
        .await
        .unwrap_or_else(|error| Err(io::Error::other(error)));
        if receiver.has_changed().unwrap_or(false) {
            continue;
        }
        let available = primary.update(result, &outputs);
        next = Instant::now() + if available { REFRESH } else { RETRY };
        let snapshot = Snapshot {
            outputs: live_outputs(outputs),
            primary: primary.known.clone(),
        };
        if previous.as_ref() != Some(&snapshot) {
            previous = Some(snapshot.clone());
            if !publish(snapshot) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests;
