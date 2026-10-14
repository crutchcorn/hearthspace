use std::{path::Path, time::Duration};

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
    .map_err(|error| format!("Could not confirm the command: {error}"))?;
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
}
