//! A minimal Xilem-based shell client.
//!
//! This is the first stage of the Xilem/Masonry integration spike: a single
//! shell control that used to be a GPUI button in [`super::bar`] is rendered
//! here by Xilem instead, running as its own Wayland client inside Hearthspace.
//! Xilem (and the Masonry widgets underneath it) own their winit window and
//! render pipeline, so the only thing this surface shares with the rest of the
//! shell is the command socket it writes to.

use std::{env, io::Write, os::unix::net::UnixStream, path::PathBuf};

use xilem::{
    EventLoop, WidgetView, WindowOptions, Xilem,
    view::{flex_col, label, text_button},
};

use crate::{config::SHELL_COMMAND_SOCKET_ENV, shell::ShellCommand};

struct XilemButton {
    command_socket: PathBuf,
}

fn app_logic(_state: &mut XilemButton) -> impl WidgetView<XilemButton> + use<> {
    flex_col((
        label("Xilem shell control"),
        text_button("Zoom In", |state: &mut XilemButton| {
            send_command(&state.command_socket, &ShellCommand::ZoomIn.wire_name());
        }),
    ))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command_socket = env::var_os(SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{SHELL_COMMAND_SOCKET_ENV} is not set"))?;

    let app = Xilem::new_simple(
        XilemButton { command_socket },
        app_logic,
        WindowOptions::new("Hearthspace Xilem Control"),
    );
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

fn send_command(command_socket: &PathBuf, command: &str) {
    match UnixStream::connect(command_socket).and_then(|mut stream| {
        stream.write_all(command.as_bytes())?;
        stream.write_all(b"\n")
    }) {
        Ok(()) => {}
        Err(error) => eprintln!("failed to send shell command {command:?}: {error}"),
    }
}
