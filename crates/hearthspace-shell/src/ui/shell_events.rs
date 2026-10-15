//! Private compositor-to-shell requests. The compositor acknowledges each token
//! again before it can open UI, so queued requests cannot survive lock or VT loss.
use std::{
    io,
    os::unix::{fs::FileTypeExt, fs::MetadataExt, fs::PermissionsExt, net::UnixDatagram},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use hearthspace_common::shell_event_socket_path;
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

    fn pending(&self) -> Vec<ShellRequest> {
        // Read one extra byte so truncated oversized datagrams are rejected.
        let mut bytes = [0; 4097];
        let mut pending: Vec<ShellRequest> = Vec::new();
        for _ in 0..32 {
            match self.socket.recv(&mut bytes) {
                Ok(length) => {
                    if let Some(request) = parse_request(&bytes[..length]) {
                        if request.control.is_none() {
                            pending.retain(|previous| previous.control.is_some());
                        }
                        pending.push(request);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    tracing::warn!(%error, "shell event receive failed");
                    break;
                }
            }
        }
        pending
    }
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
        |proxy: MessageProxy<ShellRequest>, state: &mut ShellState| {
            let events = state.shell_events.clone();
            async move {
                let Some(events) = events else {
                    return;
                };
                loop {
                    for request in events.pending() {
                        if proxy.message(request).is_err() {
                            return;
                        }
                    }
                    xilem::tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        },
        apply_request,
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
