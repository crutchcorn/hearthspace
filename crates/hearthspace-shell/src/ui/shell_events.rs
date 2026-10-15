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

    fn latest(&self) -> Option<SearchRequest> {
        // Read one extra byte so truncated oversized datagrams are rejected.
        let mut bytes = [0; 4097];
        let mut latest = None;
        for _ in 0..32 {
            match self.socket.recv(&mut bytes) {
                Ok(length) => {
                    if let Some(request) = parse_request(&bytes[..length]) {
                        latest = Some(request);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    tracing::warn!(%error, "shell event receive failed");
                    break;
                }
            }
        }
        latest
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
struct SearchRequest {
    token: u64,
    toggle: bool,
    output: Option<String>,
}

fn parse_request(bytes: &[u8]) -> Option<SearchRequest> {
    if bytes.len() > 4096 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if value["version"].as_u64()? != 1 {
        return None;
    }
    let toggle = match value["action"].as_str()? {
        "open-search" => false,
        "toggle-search" => true,
        _ => return None,
    };
    let token = value["token"].as_u64().filter(|token| *token > 0)?;
    let output = match value.get("output")? {
        serde_json::Value::Null => None,
        serde_json::Value::String(output) if !output.is_empty() && output.len() <= 256 => {
            Some(output.clone())
        }
        _ => return None,
    };
    Some(SearchRequest {
        token,
        toggle,
        output,
    })
}

pub(super) fn shell_events_task()
-> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<SearchRequest>, state: &mut ShellState| {
            let events = state.shell_events.clone();
            async move {
                let Some(events) = events else {
                    return;
                };
                loop {
                    if let Some(request) = events.latest()
                        && proxy.message(request).is_err()
                    {
                        break;
                    }
                    xilem::tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        },
        apply_request,
    )
}

fn apply_request(state: &mut ShellState, request: SearchRequest) {
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
mod tests {
    use super::*;

    #[test]
    fn events_validate_version_action_token_output_and_size() {
        let valid = br#"{"version":1,"action":"toggle-search","token":2,"output":"eDP-1"}"#;
        assert_eq!(
            parse_request(valid),
            Some(SearchRequest {
                token: 2,
                toggle: true,
                output: Some("eDP-1".into())
            })
        );
        for invalid in [
            br#"{"version":2,"action":"toggle-search","token":2,"output":null}"#.as_slice(),
            br#"{"version":1,"action":"quit","token":2,"output":null}"#,
            br#"{"version":1,"action":"open-search","token":0,"output":null}"#,
            br#"{"version":1,"action":"open-search","token":2,"output":12}"#,
            br#"{"version":1,"action":"open-search","token":2}"#,
        ] {
            assert!(parse_request(invalid).is_none());
        }
        assert!(parse_request(&vec![b' '; 4097]).is_none());
    }

    #[test]
    fn search_requests_require_current_token_and_open_is_idempotent() {
        use std::{
            io::{Read, Write},
            os::unix::net::UnixListener,
            thread,
        };
        let dir = tempfile::tempdir().unwrap();
        let command = dir.path().join("control.sock");
        let listener = UnixListener::bind(&command).unwrap();
        let server = thread::spawn(move || {
            for token in 1..=4 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                stream.read_to_string(&mut request).unwrap();
                assert_eq!(request, format!("shell-event-accept {token}\n"));
                stream
                    .write_all(if token == 1 {
                        b"err stale token\n"
                    } else {
                        b"ok\n"
                    })
                    .unwrap();
            }
        });
        let mut state = ShellState::new(command, vec!["eDP-1".into(), "HDMI-A-1".into()]);
        let mut request = SearchRequest {
            token: 1,
            toggle: false,
            output: Some("HDMI-A-1".into()),
        };
        apply_request(&mut state, request.clone());
        assert!(!state.outputs[1].show_search);
        request.token = 2;
        apply_request(&mut state, request.clone());
        assert!(state.outputs[1].show_search);
        assert_eq!(state.outputs[1].search_request_token, Some(2));
        assert!(state.outputs[1].launcher_app_id().ends_with(".request-2"));
        state.change_query("HDMI-A-1", "keep this query".into());
        request.token = 3;
        apply_request(&mut state, request.clone());
        assert_eq!(state.outputs[1].query, "keep this query");
        assert_eq!(state.outputs[1].search_request_token, Some(2));
        request.token = 4;
        request.toggle = true;
        apply_request(&mut state, request);
        assert!(!state.outputs[1].show_search);
        assert_eq!(state.outputs[1].search_request_token, None);
        state.toggle_search("HDMI-A-1");
        assert_eq!(state.outputs[1].search_request_token, None);
        assert!(!state.outputs[1].launcher_app_id().contains(".request-"));
        assert!(!state.outputs[0].show_search);
        server.join().unwrap();
    }

    #[test]
    fn listener_rejects_duplicates_and_only_removes_its_own_socket() {
        let dir = tempfile::tempdir().unwrap();
        let command = dir.path().join("control.sock");
        let events = ShellEvents::bind(&command).unwrap();
        let path = shell_event_socket_path(&command);
        assert_eq!(path.metadata().unwrap().mode() & 0o777, 0o600);
        assert!(ShellEvents::bind(&command).is_err());
        std::fs::remove_file(&path).unwrap();
        let replacement = UnixDatagram::bind(&path).unwrap();
        drop(events);
        assert!(path.exists());
        drop(replacement);
    }

    #[test]
    fn listener_recovers_stale_socket_but_preserves_regular_files_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let command = dir.path().join("control.sock");
        let path = shell_event_socket_path(&command);
        drop(UnixDatagram::bind(&path).unwrap());
        let events = ShellEvents::bind(&command).unwrap();
        drop(events);
        std::fs::write(&path, b"keep me").unwrap();
        assert!(ShellEvents::bind(&command).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        std::fs::remove_file(&path).unwrap();
        let other = dir.path().join("other.sock");
        drop(UnixDatagram::bind(&other).unwrap());
        std::os::unix::fs::symlink(&other, &path).unwrap();
        assert!(ShellEvents::bind(&command).is_err());
        assert!(path.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(other.exists());
    }

    #[test]
    fn listener_drains_to_latest_valid_request_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let command = dir.path().join("control.sock");
        let events = ShellEvents::bind(&command).unwrap();
        let path = shell_event_socket_path(&command);
        let sender = UnixDatagram::unbound().unwrap();
        for token in [1, 2] {
            let bytes =
                format!(r#"{{"version":1,"action":"open-search","token":{token},"output":null}}"#);
            sender.send_to(bytes.as_bytes(), &path).unwrap();
        }
        sender.send_to(b"bad json", &path).unwrap();
        assert_eq!(events.latest().unwrap().token, 2);
        assert!(events.latest().is_none());
        drop(events);
        assert!(!path.exists());
    }
}
