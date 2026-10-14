//! Hearthspace's Xilem-rendered UI client.
//!
//! The shell runs as its own Wayland client. It creates one control-bar surface
//! per active output, launcher palette surfaces for active searches, and a tiny
//! hidden watcher surface that keeps the output list live without disturbing the
//! visible bars' accessibility trees.

use std::{env, path::PathBuf};

use hearthspace_common::SHELL_COMMAND_SOCKET_ENV;
use tracing::{error, info};
use xilem::{EventLoop, Xilem};

mod command_client;
mod constants;
mod launcher;
mod output_watcher;
mod search_focus;
mod session_controls;
mod state;
mod views;
mod window_catalog;

use output_watcher::initial_wayland_output_names;
use state::ShellState;
use views::app_logic;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command_socket = env::var_os(SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{SHELL_COMMAND_SOCKET_ENV} is not set"))?;
    info!(path = %command_socket.display(), "starting Xilem shell UI");

    let output_names = initial_wayland_output_names().unwrap_or_else(|error| {
        error!(%error, "failed to read initial Wayland outputs; using primary shell fallback");
        Vec::new()
    });
    let state = ShellState::new(command_socket, output_names);

    let app = Xilem::new(state, app_logic)
        .with_default_properties(hearthspace_ui_components::hearthspace_light_properties())
        .with_default_base_color(hearthspace_ui_components::APP_BACKGROUND);
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
