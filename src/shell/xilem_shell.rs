//! The Hearthspace shell, rendered with Xilem.
//!
//! This is the Xilem replacement for the former GPUI shell bar. It runs as its
//! own Wayland client and hosts the app launcher (a search field plus result
//! buttons) and the compositor control buttons (pan/zoom/log).
//!
//! The window advertises [`SHELL_BAR_APP_ID`] as its Wayland `app_id` via
//! [`xilem::WindowOptionsExtLinux::with_name`] (a setter we added to our Xilem
//! fork). The compositor recognizes that id and treats the surface as shell
//! chrome — positioned as the bar and drawn *without* server-side window
//! decorations. Client-side decorations are disabled with
//! [`WindowOptions::with_decorations`] so winit does not draw a titlebar either,
//! leaving the shell completely chrome-less.

use std::{env, io::Write, os::unix::net::UnixStream, path::PathBuf};

use xilem::{
    AnyWidgetView, EventLoop, WidgetView, WindowOptions, WindowOptionsExtLinux, Xilem,
    dpi::LogicalSize,
    view::{flex_row, text_button, text_input},
};

use crate::{
    config::{CONTROL_BAR_HEIGHT, SHELL_BAR_APP_ID, SHELL_COMMAND_SOCKET_ENV},
    shell::{ShellCommand, app_catalog::AppCatalog},
};

/// Maximum number of app search results to surface at once.
const MAX_RESULTS: usize = 4;

/// Reactive state backing the shell view.
struct ShellState {
    command_socket: PathBuf,
    catalog: AppCatalog,
    query: String,
}

fn app_logic(state: &mut ShellState) -> impl WidgetView<ShellState> + use<> {
    let search = text_input(state.query.clone(), |state: &mut ShellState, value: String| {
        state.query = value;
    })
    .placeholder("Search apps...")
    .on_enter(|state: &mut ShellState, value: String| {
        if let Some(app) = state.catalog.search(value.trim(), 1).into_iter().next() {
            launch_app(&state.command_socket, &app.id);
            state.query.clear();
        }
    });

    let result_buttons: Vec<Box<AnyWidgetView<ShellState>>> = state
        .catalog
        .search(state.query.trim(), MAX_RESULTS)
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

    flex_row((search, result_buttons, command_buttons))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command_socket = env::var_os(SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{SHELL_COMMAND_SOCKET_ENV} is not set"))?;

    let state = ShellState {
        command_socket,
        catalog: AppCatalog::load(),
        query: String::new(),
    };

    let app = Xilem::new_simple(
        state,
        app_logic,
        WindowOptions::new("Hearthspace Shell")
            // Tag the surface so the compositor renders it as chrome-less shell.
            .with_name(SHELL_BAR_APP_ID, SHELL_BAR_APP_ID)
            // No client-side titlebar; the compositor owns shell placement.
            .with_decorations(false)
            .with_resizable(false)
            .with_initial_inner_size(LogicalSize::new(760.0, f64::from(CONTROL_BAR_HEIGHT))),
    );
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
