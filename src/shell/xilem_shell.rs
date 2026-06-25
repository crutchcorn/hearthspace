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
    AnyWidgetView, AppState, Color, EventLoop, TextAlign, WidgetView, WindowId,
    WindowOptionsExtLinux, WindowView, Xilem,
    dpi::LogicalSize,
    masonry::{
        core::{DefaultProperties, PropertyStack, Selector},
        layout::AsUnit,
        properties::{
            Background, BorderColor, BorderWidth, CaretColor, ContentColor, CornerRadius, Gap,
            Padding, PlaceholderColor,
        },
        theme::default_property_set,
        widgets::{Button, Label, TextArea, TextInput},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexSpacer, button, flex_col, flex_row, label, sized_box, text_button,
        text_input,
    },
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

/// Horizontal padding inside each launcher result button.
const LAUNCHER_ITEM_H_PAD: f64 = 14.0;
/// Width of a launcher result label. Sized so each row spans the dropdown width
/// once the button adds [`LAUNCHER_ITEM_H_PAD`] of padding on each side, which
/// keeps the (left-justified) label flush with the rest of the palette.
const LAUNCHER_ITEM_WIDTH: f64 = LAUNCHER_WIDTH as f64 - 2.0 * LAUNCHER_ITEM_H_PAD;

/// Maximum width of the search field. The control bar is stretched to the full
/// screen width by the compositor, so the input is capped to a comfortable size
/// instead of sprawling across the whole bar.
const SEARCH_BOX_WIDTH: f64 = 420.0;

/// Fixed height of the search field. A `TextInput` greedily fills whatever
/// block (cross-axis) space the flex row offers it and top-anchors its text, so
/// left unbounded it expands to the full bar height and the text sits too high.
/// Pinning the box to a single-line height (matching the command buttons) keeps
/// the text vertically centered within the bar.
const SEARCH_BOX_HEIGHT: f64 = 34.0;

/// Horizontal inset between the bar's contents and the screen edges, so the
/// search field does not sit flush against the left side. The control bar is
/// stretched full-width by the compositor, so this is applied as fixed spacers
/// at either end of the row.
const SHELL_BAR_H_PAD: f64 = 16.0;

// Light theme palette for the shell surfaces.
/// Window background for the bar and launcher palette.
const SHELL_BG: Color = Color::from_rgb8(0xf4, 0xf4, 0xf5);
/// Primary text color (used for labels and the search field contents).
const SHELL_TEXT: Color = Color::from_rgb8(0x1f, 0x1f, 0x23);
/// Muted color for the search field placeholder.
const SHELL_PLACEHOLDER: Color = Color::from_rgb8(0x8a, 0x8a, 0x93);
/// Background revealed under a button while it is hovered.
const BUTTON_HOVER_BG: Color = Color::from_rgb8(0xe4, 0xe4, 0xe7);
/// Slightly darker background while a button is pressed.
const BUTTON_ACTIVE_BG: Color = Color::from_rgb8(0xd4, 0xd4, 0xd8);
/// Search field fill color.
const INPUT_BG: Color = Color::from_rgb8(0xff, 0xff, 0xff);
/// Search field border color while unfocused.
const INPUT_BORDER: Color = Color::from_rgb8(0xd4, 0xd4, 0xd8);
/// Search field border color while focused.
const INPUT_FOCUS_BORDER: Color = Color::from_rgb8(0x3b, 0x7e, 0xe4);

/// Light theme overrides applied to every shell widget.
///
/// Starts from Masonry's stock (dark) defaults and recolors the few widgets the
/// shell actually uses, plus restyles buttons so they read as flat list rows:
/// no fill or border until hovered, then a soft grey highlight.
fn shell_properties() -> DefaultProperties {
    let mut properties = default_property_set();

    // Buttons: flat until hovered, then a soft highlight. No border at any time.
    properties.insert::<Button, _>(Background::Color(Color::TRANSPARENT));
    properties.insert::<Button, _>(BorderWidth { width: 0.0.px() });
    properties.insert::<Button, _>(CornerRadius { radius: 6.0.px() });
    properties.insert::<Button, _>(ContentColor::new(SHELL_TEXT));
    properties.insert::<Button, _>(Padding::from_vh(8.0.px(), LAUNCHER_ITEM_H_PAD.px()));
    {
        let mut stack = PropertyStack::new();
        stack.push(
            Selector::new().with_hovered(true),
            Background::Color(BUTTON_HOVER_BG),
        );
        stack.push(
            Selector::new().with_active(true),
            Background::Color(BUTTON_ACTIVE_BG),
        );
        properties.insert_stack::<Button>(stack);
    }

    // Dark text on the light shell.
    properties.insert::<Label, _>(ContentColor::new(SHELL_TEXT));
    properties.insert::<TextArea<true>, _>(ContentColor::new(SHELL_TEXT));
    properties.insert::<TextArea<false>, _>(ContentColor::new(SHELL_TEXT));

    // Search field: white input with a subtle border that accents on focus.
    properties.insert::<TextInput, _>(Background::Color(INPUT_BG));
    properties.insert::<TextInput, _>(BorderColor { color: INPUT_BORDER });
    properties.insert::<TextInput, _>(CaretColor { color: SHELL_TEXT });
    properties.insert::<TextInput, _>(PlaceholderColor::new(SHELL_PLACEHOLDER));
    {
        let mut stack = PropertyStack::new();
        stack.push(
            Selector::new().with_focused(true),
            BorderColor {
                color: INPUT_FOCUS_BORDER,
            },
        );
        properties.insert_stack::<TextInput>(stack);
    }

    properties
}

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

    // The control bar is stretched to the full screen width by the compositor.
    // The search field is capped to a fixed, comfortable width (rather than
    // flexing to fill the bar) so it reads as a search box; the command buttons
    // keep their natural width and pack in beside it. Fixed spacers at either
    // end inset the contents from the screen edges.
    flex_row((
        FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
        sized_box(search)
            .fixed_width(SEARCH_BOX_WIDTH.px())
            .fixed_height(SEARCH_BOX_HEIGHT.px()),
        command_buttons,
        FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
    ))
}

/// The launcher palette: a vertical list of result buttons. Selecting one
/// launches the app and clears the search field, which closes the palette.
fn launcher_view(results: Vec<DesktopApp>) -> impl WidgetView<ShellState> + use<> {
    let buttons: Vec<Box<AnyWidgetView<ShellState>>> = results
        .into_iter()
        .map(|app| {
            let app_id = app.id;
            // A left-justified label fixed to the row's content width so the
            // button's (centered) child fills the row and the text sits flush
            // left. Buttons are flat until hovered (see `shell_properties`).
            button(
                sized_box(label(app.name).text_alignment(TextAlign::Start))
                    .fixed_width(LAUNCHER_ITEM_WIDTH.px()),
                move |state: &mut ShellState| {
                    launch_app(&state.command_socket, &app_id);
                    state.query.clear();
                },
            )
            .boxed()
        })
        .collect();

    // Stretch each row to the full palette width (so the hover highlight spans
    // the row) and remove the inter-row gap so the list reads as a solid menu.
    flex_col(buttons)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Gap::ZERO)
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

    let app = Xilem::new(state, app_logic)
        .with_default_properties(shell_properties())
        .with_default_base_color(SHELL_BG);
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
