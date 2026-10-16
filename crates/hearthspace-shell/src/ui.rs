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

mod battery_view;
mod canvas_map;
mod canvas_task;
mod command_client;
mod constants;
mod control_center;
mod control_osd;
mod control_slider;
mod control_state;
mod control_task;
mod control_worker;
mod favorite_actions;
mod favorite_events;
mod favorite_state;
#[cfg(test)]
mod favorite_tests;
mod favorite_view;
use crate::gesture_pad;
mod icons;
mod launcher;
mod minimap;
mod notification_view;
mod notifications;
mod output_state;
mod output_watcher;
mod popup;
mod popup_group;
mod power_task;
mod search_editor;
mod search_focus;
mod search_input;
mod search_result;
mod search_selection;
mod session_controls;
mod shell_events;
mod state;
#[cfg(test)]
mod test_support;
mod tray_button;
mod tray_icon_loader;
mod tray_paint;
mod tray_state;
mod tray_task;
mod tray_view;
mod tray_widget;
mod views;
mod window_catalog;
mod window_group;

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
    let events = shell_events::ShellEvents::bind(&command_socket)?;
    let mut state = ShellState::new(command_socket, output_names);
    state.shell_events = Some(events);
    state.favorites = favorite_state::Favorites::load();

    let app = Xilem::new(state, app_logic)
        .with_default_properties(hearthspace_ui_components::hearthspace_light_properties())
        .with_default_base_color(hearthspace_ui_components::APP_BACKGROUND);
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

#[cfg(test)]
mod tray_tests;
