use std::{
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

use tracing::{debug, error};

use crate::ShellCommand;

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
    debug!(path = %command_socket.display(), command, "sending shell command");
    let response = hearthspace_ipc::transport::request_line(
        command_socket,
        &format!("{command}\n"),
        Duration::from_millis(750),
    )
    .map_err(|error| {
        error!(path = %command_socket.display(), command, %error, "shell command transport failed");
        format!("Could not confirm the command: {error}")
    })?;
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

pub(super) fn query_windows(command_socket: &Path) -> Result<Vec<u8>, String> {
    query_payload(command_socket, "windows", Duration::from_millis(750))
        .map_err(|error| format!("Window list unavailable: {error}"))
}

fn query_payload(
    command_socket: &Path,
    command: &str,
    timeout: Duration,
) -> std::io::Result<Vec<u8>> {
    use hearthspace_ipc::transport::{MAX_RESPONSE_BYTES, connect_until, write_until};
    use std::io::{self, ErrorKind};

    let deadline = Instant::now() + timeout;
    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .ok_or_else(|| io::Error::new(ErrorKind::TimedOut, "command deadline exceeded"))
    };
    let mut stream = connect_until(command_socket, deadline)?;
    write_until(&mut stream, format!("{command}\n").as_bytes(), deadline)?;
    let mut header = Vec::new();
    loop {
        if header.len() >= 4096 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "oversized command response header",
            ));
        }
        stream.set_read_timeout(Some(remaining()?))?;
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            break;
        }
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    if let Some(message) = header.strip_prefix("err ") {
        return Err(io::Error::other(message));
    }
    let size = header
        .strip_prefix("ok ")
        .and_then(|size| size.parse::<usize>().ok())
        .filter(|size| *size <= MAX_RESPONSE_BYTES)
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                "invalid or oversized command response",
            )
        })?;
    let mut payload = vec![0; size];
    let mut received = 0;
    while received < size {
        stream.set_read_timeout(Some(remaining()?))?;
        let count = stream.read(&mut payload[received..])?;
        if count == 0 {
            return Err(ErrorKind::UnexpectedEof.into());
        }
        received += count;
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
