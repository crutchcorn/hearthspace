use std::{
    env, fs,
    io::{self, ErrorKind},
    os::unix::net::UnixListener as CommandListener,
    path::PathBuf,
    rc::Rc,
    sync::mpsc::Receiver,
};

use smithay::reexports::calloop::LoopHandle;
use tracing::{debug, error, info, warn};

use crate::config::{SHELL_COMMAND_SOCKET_ENV, SHELL_COMMAND_SOCKET_NAME};
use hearthspace_shell::ShellCommand;

use super::runtime_path;
use crate::compositor::CalloopData;

mod connection;
mod reply;
mod source;
#[cfg(test)]
mod tests;

use reply::Limits;
use source::CommandSource;

thread_local! {
    // All listeners on this compositor thread share the same memory/connection budget.
    static LIMITS: Rc<Limits> = Rc::default();
}

pub(in crate::compositor) fn command_socket_path() -> PathBuf {
    env::var_os(SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime_path(SHELL_COMMAND_SOCKET_NAME))
}

pub(in crate::compositor) fn remove_stale_socket(path: &PathBuf) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(in crate::compositor) fn accept_command_connections<'l>(
    listener: &CommandListener,
    handle: &LoopHandle<'l, CalloopData>,
) -> io::Result<()> {
    // Bound work per dispatch as well as the number of live connections.
    for _ in 0..32 {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true)?;
                let Some(source) = LIMITS.with(|limits| CommandSource::new(stream, limits.clone()))
                else {
                    warn!("command connection limit reached");
                    continue;
                };
                if let Err(error) = handle.insert_source(source, |line, _, data| {
                    run_command_line(&line, |text| data.run_command_text(text))
                }) {
                    error!(%error, "failed to register shell command connection");
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn run_command_line(bytes: &[u8], run: impl FnOnce(&str) -> ControlReply) -> ControlReply {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return ControlReply::Err("command is not valid UTF-8".into());
    };
    let text = text.trim();
    if text.is_empty() {
        return ControlReply::Err("empty command".into());
    }
    run(text)
}

pub(in crate::compositor) enum ControlReply {
    Ok,
    Payload(Vec<u8>),
    Err(String),
    /// Completed asynchronously, for example after a native page flip.
    Deferred(Receiver<ControlReply>),
}

impl CalloopData {
    fn run_command_text(&mut self, text: &str) -> ControlReply {
        if self.state.session_locked() && text != "status" && text != "quit" {
            return ControlReply::Err("session is locked".into());
        }
        if text == "clipboard-set" {
            return self.state.set_clipboard_text("");
        }
        if let Some(hex) = text.strip_prefix("clipboard-set ") {
            return self.state.set_clipboard_text(hex);
        }
        if let Some(reply) = self.run_automation_command(text) {
            return reply;
        }
        let Some(action) = ShellCommand::parse(text) else {
            return ControlReply::Err("invalid command".into());
        };
        debug!(
            command = action.wire_name(),
            "running compositor control action"
        );
        if action == ShellCommand::Quit {
            info!("quit control action requested; stopping compositor event loop");
            self.running = false;
            return ControlReply::Ok;
        }
        if action == ShellCommand::Screenshot {
            return self.request_capture(None, false);
        }
        self.state.run_control_action(action)
    }
}
