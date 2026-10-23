//! Private compositor-to-shell requests. The compositor acknowledges each token
//! again before it can open UI, so queued requests cannot survive lock or VT loss.
use std::{
    io,
    os::fd::{AsFd, BorrowedFd},
    os::unix::{fs::FileTypeExt, fs::MetadataExt, fs::PermissionsExt, net::UnixDatagram},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use hearthspace_common::shell_event_socket_path;
use tokio::{
    io::{Interest, unix::AsyncFd},
    time::{Instant, MissedTickBehavior, interval_at},
};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::{command_client::send_command, state::ShellState};

pub(super) struct ShellEvents {
    socket: UnixDatagram,
    path: PathBuf,
    identity: (u64, u64),
}

impl ShellEvents {
    pub(super) fn bind(command_socket: &Path) -> io::Result<Arc<Self>> {
        let path = shell_event_socket_path(command_socket);
        let socket = match UnixDatagram::bind(&path) {
            Ok(socket) => socket,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                remove_stale_socket(&path)?;
                UnixDatagram::bind(&path)?
            }
            Err(error) => return Err(error),
        };
        let metadata = path.symlink_metadata()?;
        let events = Self {
            socket,
            path,
            identity: (metadata.dev(), metadata.ino()),
        };
        std::fs::set_permissions(&events.path, std::fs::Permissions::from_mode(0o600))?;
        events.socket.set_nonblocking(true)?;
        Ok(Arc::new(events))
    }

    #[cfg(test)]
    fn pending(&self) -> Vec<Event> {
        self.drain().unwrap().0
    }

    /// The second result is true only after recv reports WouldBlock. Hitting
    /// the batch limit must retain readiness so a queued tail is read promptly.
    fn drain(&self) -> io::Result<(Vec<Event>, bool)> {
        // Read one extra byte so truncated oversized datagrams are rejected.
        let mut bytes = [0; 4097];
        let mut pending: Vec<Event> = Vec::new();
        for _ in 0..32 {
            match self.socket.recv(&mut bytes) {
                Ok(length) => {
                    if let Some(request) = parse_event(&bytes[..length]) {
                        if matches!(&request, Event::Request(request) if request.control.is_none())
                        {
                            pending.retain(|previous| {
                                !matches!(previous,
                                Event::Request(request) if request.control.is_none())
                            });
                        }
                        if matches!(request, Event::Desktop(super::desktop::Event::Changed)) {
                            pending.retain(|previous| {
                                !matches!(previous, Event::Desktop(super::desktop::Event::Changed))
                            });
                        }
                        if matches!(request, Event::CanvasSearchChanged) {
                            pending
                                .retain(|previous| !matches!(previous, Event::CanvasSearchChanged));
                        }
                        pending.push(request);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok((pending, true));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok((pending, false))
    }
}

async fn receive(events: &ShellEvents, socket: &AsyncFd<BorrowedFd<'_>>) -> io::Result<Vec<Event>> {
    let mut ready = socket.readable().await?;
    let (pending, drained) = events.drain()?;
    if drained {
        ready.clear_ready();
    }
    Ok(pending)
}

fn remove_stale_socket(path: &Path) -> io::Result<()> {
    let metadata = path.symlink_metadata()?;
    let own_uid = std::fs::metadata("/proc/self")?.uid();
    let in_use = || {
        io::Error::new(
            io::ErrorKind::AddrInUse,
            "shell event socket is already owned",
        )
    };
    if !metadata.file_type().is_socket() || metadata.uid() != own_uid {
        return Err(in_use());
    }
    // Connecting a datagram socket does not send any data to a live shell.
    // A refused connection proves the filesystem endpoint has no listener.
    match UnixDatagram::unbound()?.connect(path) {
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {}
        _ => return Err(in_use()),
    }
    let current = path.symlink_metadata()?;
    if (current.dev(), current.ino()) != (metadata.dev(), metadata.ino()) {
        return Err(in_use());
    }
    std::fs::remove_file(path)
}

impl Drop for ShellEvents {
    fn drop(&mut self) {
        if self
            .path
            .symlink_metadata()
            .is_ok_and(|metadata| (metadata.dev(), metadata.ino()) == self.identity)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShellRequest {
    token: u64,
    toggle: bool,
    control: Option<super::control_state::Action>,
    output: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Request(ShellRequest),
    Gesture(super::favorite_events::GestureEvent),
    Desktop(super::desktop::Event),
    CanvasSearchChanged,
    CanvasSearchAction(u64),
    RetryDrawing,
    ReloadFavorites,
}

fn parse_event(bytes: &[u8]) -> Option<Event> {
    if bytes.len() > 4096 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if value["version"].as_u64()? != 1 {
        return None;
    }
    if value["action"] == "canvas-search-changed" {
        Some(Event::CanvasSearchChanged)
    } else if value["action"] == "canvas-search-action" {
        value["token"]
            .as_u64()
            .filter(|token| *token > 0)
            .map(Event::CanvasSearchAction)
    } else if let Some(event) = super::desktop::parse_event(&value) {
        Some(Event::Desktop(event))
    } else if value["action"].as_str()? == "favorite-gesture" {
        super::favorite_events::parse(&value).map(Event::Gesture)
    } else {
        parse_request(bytes).map(Event::Request)
    }
}

fn parse_request(bytes: &[u8]) -> Option<ShellRequest> {
    if bytes.len() > 4096 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if value["version"].as_u64()? != 1 {
        return None;
    }
    let toggle = match value["action"].as_str()? {
        "open-search" | "device-control" => false,
        "toggle-search" => true,
        _ => return None,
    };
    let control = if value["action"] == "device-control" {
        Some(super::control_state::Action::from_key(
            value["control"].as_str()?,
        )?)
    } else {
        None
    };
    let token = value["token"].as_u64().filter(|token| *token > 0)?;
    if control.is_some() != (token & (1 << 63) != 0) {
        return None;
    }
    let output = match value.get("output")? {
        serde_json::Value::Null => None,
        serde_json::Value::String(output) if !output.is_empty() && output.len() <= 256 => {
            Some(output.clone())
        }
        _ => return None,
    };
    Some(ShellRequest {
        token,
        toggle,
        control,
        output,
    })
}

pub(super) fn shell_events_task()
-> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Event>, state: &mut ShellState| {
            let events = state.shell_events.clone();
            async move {
                let Some(events) = events else {
                    return;
                };
                // SAFETY: events owns a stable, open descriptor for this entire
                // task. This local registration is never forgotten or moved out,
                // and is dropped before the Arc owning the socket on every exit.
                let registration = unsafe {
                    AsyncFd::register_with_interest(events.socket.as_fd(), Interest::READABLE)
                };
                let socket = match registration {
                    Ok(socket) => socket,
                    Err(error) => {
                        tracing::warn!(%error, "cannot register shell event socket readiness");
                        return;
                    }
                };
                let retry_period = Duration::from_millis(250);
                let reload_period = Duration::from_secs(1);
                let mut retry = interval_at(Instant::now() + retry_period, retry_period);
                let mut reload = interval_at(Instant::now() + reload_period, reload_period);
                retry.set_missed_tick_behavior(MissedTickBehavior::Skip);
                reload.set_missed_tick_behavior(MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        pending = receive(&events, &socket) => {
                            let pending = match pending {
                                Ok(pending) => pending,
                                Err(error) => {
                                    tracing::warn!(%error, "shell event socket readiness failed");
                                    return;
                                }
                            };
                            for request in pending {
                                if proxy.message(request).is_err() { return; }
                            }
                        }
                        _ = retry.tick() => {
                            if proxy.message(Event::RetryDrawing).is_err() { return; }
                        }
                        _ = reload.tick() => {
                            if proxy.message(Event::ReloadFavorites).is_err() { return; }
                        }
                    }
                }
            }
        },
        |state, event| match event {
            Event::Request(request) => apply_request(state, request),
            Event::Gesture(event) => super::favorite_events::apply(state, event),
            Event::Desktop(event) => super::desktop::apply_event(state, event),
            Event::CanvasSearchChanged => state.canvas_search.refresh.notify_one(),
            Event::CanvasSearchAction(token) => super::canvas_search::submit_command(
                state,
                format!("canvas-search-event-accept {token}"),
            ),
            Event::RetryDrawing => super::favorite_events::retry(state),
            Event::ReloadFavorites => super::favorite_events::reload(state),
        },
    )
}

fn apply_request(state: &mut ShellState, request: ShellRequest) {
    let key = match request.output.as_deref() {
        Some(name) => state
            .outputs
            .iter()
            .find(|output| output.output_name.as_deref() == Some(name)),
        None => state.outputs.first(),
    }
    .map(|output| output.key().to_string());
    let Some(key) = key else {
        return;
    };
    if let Some(action) = request.control {
        super::control_task::submit(state, &key, action, Some(request.token));
        return;
    }
    if send_command(
        &state.command_socket,
        &format!("shell-event-accept {}", request.token),
    )
    .is_err()
    {
        return;
    }
    if request.toggle
        || !state
            .outputs
            .iter()
            .any(|output| output.key() == key && output.show_search)
    {
        state.toggle_search(&key);
        if let Some(output) = state
            .outputs
            .iter_mut()
            .find(|output| output.key() == key && output.show_search)
        {
            output.search_request_token = Some(request.token);
        }
    }
}

#[cfg(test)]
#[path = "shell_events_tests.rs"]
mod tests;
