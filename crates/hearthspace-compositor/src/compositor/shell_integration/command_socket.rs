use std::{
    env, fs,
    io::{self, ErrorKind, Read, Write},
    os::unix::net::{UnixListener as CommandListener, UnixStream},
    path::PathBuf,
};

use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, generic::Generic};
use tracing::{debug, error, info, trace, warn};

use crate::config::{SHELL_COMMAND_SOCKET_ENV, SHELL_COMMAND_SOCKET_NAME};
use hearthspace_shell::ShellCommand;

use super::runtime_path;
use crate::compositor::CalloopData;

pub(in crate::compositor) fn command_socket_path() -> PathBuf {
    if let Some(socket_path) = env::var_os(SHELL_COMMAND_SOCKET_ENV) {
        return PathBuf::from(socket_path);
    }

    runtime_path(SHELL_COMMAND_SOCKET_NAME)
}

pub(in crate::compositor) fn remove_stale_socket(path: &PathBuf) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Largest amount of unparsed command data we will buffer for a single
/// connection before giving up. Commands are short single lines, so this only
/// guards against a misbehaving client streaming data without a newline.
const MAX_COMMAND_BUFFER_BYTES: usize = 4096;

/// Accept every pending command connection and register each one as its own
/// non-blocking calloop source. Reading is incremental, so a client that
/// connects but never finishes sending cannot block the event loop.
pub(in crate::compositor) fn accept_command_connections<'l>(
    listener: &CommandListener,
    handle: &LoopHandle<'l, CalloopData>,
) -> io::Result<()> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                debug!("accepted shell command connection");
                stream.set_nonblocking(true)?;
                register_command_connection(handle, stream);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

fn register_command_connection<'l>(handle: &LoopHandle<'l, CalloopData>, stream: UnixStream) {
    let mut buffer: Vec<u8> = Vec::new();
    let source = Generic::new(stream, Interest::READ, Mode::Level);
    if let Err(error) = handle.insert_source(source, move |_, stream, data| {
        Ok(read_command_connection(stream, &mut buffer, data))
    }) {
        error!(%error, "failed to register shell command connection");
    }
}

fn read_command_connection(
    stream: &UnixStream,
    buffer: &mut Vec<u8>,
    data: &mut CalloopData,
) -> PostAction {
    // `Read` is implemented for `&UnixStream`, so read through a shared ref.
    let mut reader = stream;
    let mut chunk = [0u8; 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => {
                // Client closed: run any trailing line that lacked a newline.
                if !buffer.is_empty() {
                    run_command_line(buffer, data);
                }
                return PostAction::Remove;
            }
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if !drain_complete_commands(buffer, data, stream) {
                    return PostAction::Remove;
                }
                if buffer.len() > MAX_COMMAND_BUFFER_BYTES {
                    warn!(
                        limit = MAX_COMMAND_BUFFER_BYTES,
                        "shell command exceeded maximum buffered length; dropping connection"
                    );
                    return PostAction::Remove;
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == ErrorKind::WouldBlock => return PostAction::Continue,
            Err(error) => {
                error!(%error, "failed to read shell command");
                return PostAction::Remove;
            }
        }
    }
}

fn drain_complete_commands(
    buffer: &mut Vec<u8>,
    data: &mut CalloopData,
    stream: &UnixStream,
) -> bool {
    for command in take_complete_commands(buffer) {
        if !write_control_reply(stream, data.run_control_action(command)) {
            return false;
        }
    }
    true
}

/// Split every complete newline-terminated command off the front of `buffer`,
/// returning the parsed commands in order. Unterminated trailing bytes are left
/// in `buffer` for the next read. Lines that fail to parse are silently skipped.
fn take_complete_commands(buffer: &mut Vec<u8>) -> Vec<ShellCommand> {
    let mut commands = Vec::new();
    while let Some(newline) = buffer.iter().position(|&byte| byte == b'\n') {
        let line: Vec<u8> = buffer.drain(..=newline).collect();
        if let Some(command) = parse_command_line(&line) {
            commands.push(command);
        }
    }
    commands
}

fn run_command_line(bytes: &[u8], data: &mut CalloopData) {
    if let Some(command) = parse_command_line(bytes) {
        trace!(command = %command.wire_name(), "running trailing shell command line");
        data.run_control_action(command);
    }
}

fn parse_command_line(bytes: &[u8]) -> Option<ShellCommand> {
    let text = std::str::from_utf8(bytes).ok()?;
    let trimmed = text.trim();
    let command = ShellCommand::parse(trimmed);
    if command.is_none() && !trimmed.is_empty() {
        warn!(line = trimmed, "ignored invalid shell command");
    }
    command
}

pub(in crate::compositor) enum ControlReply {
    Ok,
    Payload(Vec<u8>),
    Err(String),
}

impl ControlReply {
    fn bytes(&self) -> Vec<u8> {
        match self {
            Self::Ok => b"ok\n".to_vec(),
            Self::Payload(payload) => {
                let mut bytes = format!("ok {}\n", payload.len()).into_bytes();
                bytes.extend_from_slice(payload);
                bytes
            }
            Self::Err(message) => format!("err {message}\n").into_bytes(),
        }
    }
}

fn write_control_reply(stream: &UnixStream, reply: ControlReply) -> bool {
    let mut writer = stream;
    match writer.write_all(&reply.bytes()) {
        Ok(()) => true,
        Err(error) if matches!(error.kind(), ErrorKind::BrokenPipe | ErrorKind::WouldBlock) => true,
        Err(error) => {
            error!(%error, "failed to write shell command reply");
            false
        }
    }
}

impl CalloopData {
    fn run_control_action(&mut self, action: ShellCommand) -> ControlReply {
        let command = action.wire_name();
        debug!(%command, "running compositor control action");
        if action == ShellCommand::Quit {
            info!("quit control action requested; stopping compositor event loop");
            self.running = false;
            return ControlReply::Ok;
        }

        if action == ShellCommand::Screenshot {
            return match self.screenshot_png() {
                Ok(bytes) => ControlReply::Payload(bytes),
                Err(error) => ControlReply::Err(format!("screenshot failed: {error}")),
            };
        }

        self.state.run_control_action(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearthspace_shell::SpawnTarget;

    #[test]
    fn drains_only_complete_newline_terminated_commands() {
        let mut buffer = b"zoom-in\npan-left\n".to_vec();
        let commands = take_complete_commands(&mut buffer);
        assert_eq!(commands, vec![ShellCommand::ZoomIn, ShellCommand::PanLeft]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn keeps_an_unterminated_trailing_command_buffered() {
        let mut buffer = b"zoom-in\npan-le".to_vec();
        let commands = take_complete_commands(&mut buffer);
        assert_eq!(commands, vec![ShellCommand::ZoomIn]);
        assert_eq!(buffer, b"pan-le");

        // The remainder completes once the newline arrives.
        buffer.extend_from_slice(b"ft\n");
        let commands = take_complete_commands(&mut buffer);
        assert_eq!(commands, vec![ShellCommand::PanLeft]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn skips_unparseable_lines_while_draining() {
        let mut buffer = b"not-a-command\nspawn foot\n".to_vec();
        let commands = take_complete_commands(&mut buffer);
        assert_eq!(commands, vec![ShellCommand::Spawn(SpawnTarget::Foot)]);
    }

    #[test]
    fn parse_command_line_trims_and_rejects_invalid_utf8() {
        assert_eq!(
            parse_command_line(b"  zoom-out \n"),
            Some(ShellCommand::ZoomOut)
        );
        assert_eq!(parse_command_line(&[0xff, 0xfe]), None);
        assert_eq!(parse_command_line(b"\n"), None);
    }
}
