//! The Hearthspace shell, rendered with Xilem.
//!
//! The shell runs as its own Wayland client and hosts two surfaces:
//!
//! - the **control bar**: a search field plus the compositor control buttons
//!   (pan/zoom/log), pinned full-width across the top of the screen, and
//! - the **launcher palette**: a transient dropdown opened just below the bar
//!   that lists app-search results while the user is typing.
//!
//! Each window advertises a distinct Wayland `app_id` ([`SHELL_BAR_APP_ID`] and
//! [`LAUNCHER_APP_ID`]) via [`xilem::WindowOptionsExtLinux::with_name`] (a setter
//! we added to our Xilem fork). The compositor recognizes those ids and treats
//! the surfaces as shell chrome — positioned and drawn *without* server-side
//! window decorations. Client-side decorations are disabled with
//! [`WindowOptions::with_decorations`] so winit does not draw a titlebar either,
//! leaving the shell completely chrome-less.

use std::{env, io::Write, os::unix::net::UnixStream, path::PathBuf};

use xilem::{
    AnyWidgetView, AppState, EventLoop, WidgetView, WindowId, WindowOptionsExtLinux, WindowView,
    Xilem,
    dpi::LogicalSize,
    view::{FlexExt, flex_col, flex_row, text_button, text_input},
    window,
};

use crate::{
    config::{CONTROL_BAR_HEIGHT, LAUNCHER_APP_ID, SHELL_BAR_APP_ID, SHELL_COMMAND_SOCKET_ENV},
    shell::{
        ShellCommand,
        app_catalog::{AppCatalog, DesktopApp},
    },
};

/// Maximum number of app search results to surface at once.
const MAX_RESULTS: usize = 4;

/// Width of the launcher palette dropdown, in logical pixels.
const LAUNCHER_WIDTH: i32 = 360;
/// Approximate height of a single result row, used to size the dropdown to its
/// contents.
const LAUNCHER_ROW_HEIGHT: f64 = 40.0;
/// Vertical padding added around the launcher result list.
const LAUNCHER_PADDING: f64 = 12.0;

/// Inner size of the launcher dropdown for a given number of results. The
/// dropdown is sized to its contents so it reads as a snug menu.
fn launcher_size(result_count: usize) -> LogicalSize<f64> {
    LogicalSize::new(
        f64::from(LAUNCHER_WIDTH),
        result_count as f64 * LAUNCHER_ROW_HEIGHT + LAUNCHER_PADDING,
    )
}

/// Reactive state backing the shell view.
struct ShellState {
    command_socket: PathBuf,
    catalog: AppCatalog,
    query: String,
    running: bool,
    /// Stable window ids: the bar is always present, while the launcher palette
    /// is only opened while there are search results to show.
    bar_window_id: WindowId,
    launcher_window_id: WindowId,
}

impl AppState for ShellState {
    fn keep_running(&self) -> bool {
        self.running
    }
}

/// Search results to show in the launcher dropdown. Returns nothing while the
/// search field is empty so the palette stays closed until the user types.
fn search_results(state: &ShellState) -> Vec<DesktopApp> {
    let query = state.query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    state.catalog.search(query, MAX_RESULTS)
}

/// The control bar: the search field plus the compositor control buttons.
fn bar_view(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    let search = text_input(
        state.query.clone(),
        |state: &mut ShellState, value: String| {
            state.query = value;
        },
    )
    .placeholder("Search apps...")
    .on_enter(|state: &mut ShellState, value: String| {
        let query = value.trim();
        if query.is_empty() {
            return;
        }
        if let Some(app) = state.catalog.search(query, 1).into_iter().next() {
            launch_app(&state.command_socket, &app.id);
            state.query.clear();
        }
    });

    let command_buttons: Vec<Box<AnyWidgetView<ShellState>>> = [
        ShellCommand::PanLeft,
        ShellCommand::PanRight,
        ShellCommand::PanUp,
        ShellCommand::PanDown,
        ShellCommand::ZoomIn,
        ShellCommand::ZoomOut,
        ShellCommand::LogAccessibilityTree,
    ]
    .into_iter()
    .map(|command| {
        text_button(command.label(), move |state: &mut ShellState| {
            send_command(&state.command_socket, &command.wire_name());
        })
        .boxed()
    })
    .collect();

    // The compositor forces the bar to a single full-width, short row, so the
    // search field is given a flex factor to claim all leftover horizontal space
    // (the command buttons keep their natural width). Without this the row splits
    // space evenly and the input collapses to an untypeable sliver.
    flex_row((search.flex(1.0), command_buttons))
}

/// The launcher palette: a vertical list of result buttons. Selecting one
/// launches the app and clears the search field, which closes the palette.
fn launcher_view(results: Vec<DesktopApp>) -> impl WidgetView<ShellState> + use<> {
    let buttons: Vec<Box<AnyWidgetView<ShellState>>> = results
        .into_iter()
        .map(|app| {
            let app_id = app.id;
            text_button(app.name, move |state: &mut ShellState| {
                launch_app(&state.command_socket, &app_id);
                state.query.clear();
            })
            .boxed()
        })
        .collect();

    flex_col(buttons)
}

fn app_logic(state: &mut ShellState) -> impl Iterator<Item = WindowView<ShellState>> + use<> {
    let bar = window(state.bar_window_id, "Hearthspace Shell", bar_view(state)).with_options(
        |options| {
            options
                // Tag the surface so the compositor renders it as chrome-less shell.
                .with_name(SHELL_BAR_APP_ID, SHELL_BAR_APP_ID)
                // No client-side titlebar; the compositor owns shell placement.
                .with_decorations(false)
                .with_resizable(false)
                .with_initial_inner_size(LogicalSize::new(760.0, f64::from(CONTROL_BAR_HEIGHT)))
                .on_close(|state: &mut ShellState| state.running = false)
        },
    );

    let results = search_results(state);
    let launcher_id = state.launcher_window_id;
    let launcher = (!results.is_empty()).then(|| {
        let size = launcher_size(results.len());
        window(launcher_id, "Hearthspace Launcher", launcher_view(results)).with_options(
            move |options| {
                options
                    .with_name(LAUNCHER_APP_ID, LAUNCHER_APP_ID)
                    .with_decorations(false)
                    .with_resizable(false)
                    // The initial size must stay constant across rebuilds (winit
                    // cannot change it afterwards), so the dropdown is grown and
                    // shrunk to its contents through the reactive min/max size.
                    .with_initial_inner_size(launcher_size(MAX_RESULTS))
                    .with_min_inner_size(size)
                    .with_max_inner_size(size)
            },
        )
    });

    std::iter::once(bar).chain(launcher)
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command_socket = env::var_os(SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{SHELL_COMMAND_SOCKET_ENV} is not set"))?;

    let state = ShellState {
        command_socket,
        catalog: AppCatalog::load(),
        query: String::new(),
        running: true,
        bar_window_id: WindowId::next(),
        launcher_window_id: WindowId::next(),
    };

    let app = Xilem::new(state, app_logic);
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

fn launch_app(command_socket: &PathBuf, app_id: &str) {
    send_command(
        command_socket,
        &ShellCommand::LaunchApp(app_id.to_string()).wire_name(),
    );
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
