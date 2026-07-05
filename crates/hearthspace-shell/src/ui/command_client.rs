use std::{io::Write, os::unix::net::UnixStream, path::PathBuf};

use tracing::{debug, error};

use crate::ShellCommand;

pub(super) fn launch_app(command_socket: &PathBuf, app_id: &str) {
    send_command(
        command_socket,
        &ShellCommand::LaunchApp(app_id.to_string()).wire_name(),
    );
}

pub(super) fn send_command(command_socket: &PathBuf, command: &str) {
    debug!(path = %command_socket.display(), command, "sending shell command");
    match UnixStream::connect(command_socket).and_then(|mut stream| {
        stream.write_all(command.as_bytes())?;
        stream.write_all(b"\n")
    }) {
        Ok(()) => {}
        Err(error) => {
            error!(path = %command_socket.display(), command, %error, "failed to send shell command")
        }
    }
}
