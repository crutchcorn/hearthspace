use std::{
    path::Path,
    time::{Duration, Instant},
};

use tracing::{debug, error};

use crate::ShellCommand;
use hearthspace_ipc::transport::launch::Cancellation;

pub(super) fn launch_app(
    command_socket: &Path,
    app_id: &str,
    output_name: Option<&str>,
) -> Result<(), String> {
    let command = match output_name {
        Some(output_name) => ShellCommand::LaunchAppOnOutput {
            app_id: app_id.to_string(),
            output_name: output_name.to_string(),
        },
        None => ShellCommand::LaunchApp(app_id.to_string()),
    };
    send_command(command_socket, &command.wire_name())
}

pub(super) fn send_command(command_socket: &Path, command: &str) -> Result<(), String> {
    send_command_with_timeout(command_socket, command, command_timeout(command))
}

pub(super) fn send_command_with_timeout(
    command_socket: &Path,
    command: &str,
    timeout: Duration,
) -> Result<(), String> {
    debug!(path = %command_socket.display(), command, "sending shell command");
    let response = hearthspace_ipc::transport::request_line(
        command_socket,
        &format!("{command}\n"),
        timeout,
    )
    .map_err(|error| {
        error!(path = %command_socket.display(), command, %error, "shell command transport failed");
        format!("Could not confirm the command: {error}")
    })?;
    parse_reply(command_socket, command, &response)
}

pub(super) fn send_launch_until(
    command_socket: &Path,
    command: &str,
    deadline: Instant,
    cancelled: &Cancellation,
) -> Result<(), String> {
    let response =
        hearthspace_ipc::transport::launch::request(command_socket, command, deadline, cancelled)
            .map_err(|error| format!("Could not confirm the command: {error}"))?;
    parse_reply(command_socket, command, &response)
}

fn parse_reply(command_socket: &Path, command: &str, response: &str) -> Result<(), String> {
    match response.trim_end() {
        "ok" => Ok(()),
        response => {
            let error = response
                .strip_prefix("err ")
                .unwrap_or("Invalid compositor response");
            error!(path = %command_socket.display(), command, %error, "shell command failed");
            Err(error.to_string())
        }
    }
}

fn command_timeout(command: &str) -> Duration {
    let deferred = matches!(
        crate::ShellCommand::parse(command),
        Some(
            crate::ShellCommand::LaunchApp(_)
                | crate::ShellCommand::LaunchAppOnOutput { .. }
                | crate::ShellCommand::OpenSearchSettings { .. }
        )
    ) || command.starts_with("favorite-gesture-launch ")
        || command.starts_with("desktop-context-background ");
    if deferred {
        Duration::from_secs(10)
    } else if matches!(
        crate::ShellCommand::parse(command),
        Some(crate::ShellCommand::ReloadSettings)
    ) {
        // A bounded watch may finish before the explicit GET is acknowledged.
        Duration::from_secs(2)
    } else {
        Duration::from_millis(750)
    }
}

pub(super) fn query_windows(command_socket: &Path) -> Result<Vec<u8>, String> {
    query_payload(command_socket, "windows", Duration::from_millis(750))
        .map_err(|error| format!("Window list unavailable: {error}"))
}

pub(super) fn query_canvas(command_socket: &Path) -> Result<Vec<u8>, String> {
    query_payload(command_socket, "status", Duration::from_millis(750))
        .map_err(|error| format!("Canvas map unavailable: {error}"))
}

pub(super) fn query_group(command_socket: &Path) -> Result<Vec<u8>, String> {
    query_payload(command_socket, "group-status", Duration::from_millis(750))
        .map_err(|error| format!("Window group unavailable: {error}"))
}

// A desktop may contain 4096 IDs of 1024 bytes each. JSON can double their
// escaped length; leave bounded space for selection and position metadata too.
const MAX_DESKTOP_RESPONSE_BYTES: usize = 12 * 1024 * 1024;

pub(super) fn query_desktop(command_socket: &Path) -> std::io::Result<Vec<u8>> {
    query_payload_bounded(
        command_socket,
        "desktop-status",
        Duration::from_millis(750),
        MAX_DESKTOP_RESPONSE_BYTES,
    )
}

pub(super) fn query_frame_layout(command_socket: &Path) -> std::io::Result<Vec<u8>> {
    query_payload_bounded(
        command_socket,
        "layout-status",
        Duration::from_millis(750),
        MAX_DESKTOP_RESPONSE_BYTES,
    )
}

pub(super) fn query_canvas_search(command_socket: &Path) -> std::io::Result<Vec<u8>> {
    query_payload_bounded(
        command_socket,
        "canvas-search-status",
        Duration::from_millis(750),
        MAX_DESKTOP_RESPONSE_BYTES,
    )
}

pub(super) fn query_payload(
    command_socket: &Path,
    command: &str,
    timeout: Duration,
) -> std::io::Result<Vec<u8>> {
    query_payload_bounded(
        command_socket,
        command,
        timeout,
        hearthspace_ipc::transport::MAX_RESPONSE_BYTES,
    )
}

pub(super) fn query_payload_bounded(
    command_socket: &Path,
    command: &str,
    timeout: Duration,
    max_response_bytes: usize,
) -> std::io::Result<Vec<u8>> {
    hearthspace_ipc::transport::request_payload(
        command_socket,
        command,
        timeout,
        max_response_bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        thread,
    };

    #[test]
    fn launch_waits_for_acceptance_and_reports_rejected_launches() {
        for (response, succeeds) in [
            ("ok\n", true),
            ("err missing executable\n", false),
            ("invalid\n", false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("control.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = String::new();
                stream.read_to_string(&mut request).unwrap();
                assert_eq!(request, "launch-app app.desktop\n");
                stream.write_all(response.as_bytes()).unwrap();
            });
            assert_eq!(launch_app(&path, "app.desktop", None).is_ok(), succeeds);
            server.join().unwrap();
        }
    }

    #[test]
    fn cold_app_acceptance_can_outlive_the_short_control_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).unwrap();
            assert_eq!(request, "launch-app app.desktop\n");
            thread::sleep(Duration::from_millis(850));
            stream
                .write_all(b"err application refused activation\n")
                .unwrap();
        });
        assert_eq!(
            launch_app(&path, "app.desktop", None).unwrap_err(),
            "application refused activation"
        );
        server.join().unwrap();
    }

    #[test]
    fn window_payload_survives_coalesced_header_and_fragmented_body() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let payload = br#"{"version":1,"windows":[{"id":42,"title":"Notes"}]}"#;
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 8];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(&request, b"windows\n");
            stream
                .write_all(format!("ok {}\n", payload.len()).as_bytes())
                .unwrap();
            for chunk in payload.chunks(7) {
                stream.write_all(chunk).unwrap();
            }
        });
        assert_eq!(query_windows(&path).unwrap(), payload);
        server.join().unwrap();
    }

    #[test]
    fn window_payload_rejects_oversized_or_truncated_responses() {
        for (response, expected) in [
            (b"ok 65537\n".as_slice(), std::io::ErrorKind::InvalidData),
            (b"ok 10\n{}".as_slice(), std::io::ErrorKind::UnexpectedEof),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("control.sock");
            let listener = UnixListener::bind(&path).unwrap();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 8];
                stream.read_exact(&mut request).unwrap();
                stream.write_all(response).unwrap();
            });
            let error = query_payload(&path, "windows", Duration::from_secs(1)).unwrap_err();
            assert_eq!(error.kind(), expected);
            server.join().unwrap();
        }
    }

    #[test]
    fn window_payload_times_out_when_the_server_stalls_mid_body() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 8];
            stream.read_exact(&mut request).unwrap();
            stream.write_all(b"ok 10\n{").unwrap();
            thread::sleep(Duration::from_millis(150));
        });
        let start = Instant::now();
        let error = query_payload(&path, "windows", Duration::from_millis(50)).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ));
        assert!(start.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }

    #[test]
    fn desktop_status_accepts_a_large_catalog_over_the_small_query_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let items: Vec<_> = (0..600)
            .map(|index| {
                serde_json::json!({
                    "id": format!("app:{index:04}{}.desktop", "x".repeat(80)),
                    "x": index * 104, "y": 112, "selected": index == 599,
                    "focused": index == 599, "keyboard_focus": true,
                })
            })
            .collect();
        let payload = serde_json::to_vec(&serde_json::json!({
            "version":1, "revision":4, "select_mode":true, "items":items,
        }))
        .unwrap();
        assert!(payload.len() > hearthspace_ipc::transport::MAX_RESPONSE_BYTES);
        let expected = payload.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 15];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(&request, b"desktop-status\n");
            stream
                .write_all(format!("ok {}\n", payload.len()).as_bytes())
                .unwrap();
            for chunk in payload.chunks(4093) {
                stream.write_all(chunk).unwrap();
            }
        });
        let received = query_desktop(&path).unwrap();
        assert_eq!(received, expected);
        let value: serde_json::Value = serde_json::from_slice(&received).unwrap();
        assert_eq!(value["items"].as_array().unwrap().len(), 600);
        assert_eq!(value["items"][599]["selected"], true);
        server.join().unwrap();
    }

    #[test]
    fn desktop_status_rejects_an_over_limit_header_before_allocating_the_body() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 15];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(&request, b"desktop-status\n");
            stream
                .write_all(format!("ok {}\n", MAX_DESKTOP_RESPONSE_BYTES + 1).as_bytes())
                .unwrap();
        });
        assert_eq!(
            query_desktop(&path).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        server.join().unwrap();
    }
}
