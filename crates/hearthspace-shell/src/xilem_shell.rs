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

use std::{env, io, io::Write, os::unix::net::UnixStream, path::PathBuf};

use tracing::{debug, error, info};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_output, wl_registry},
};
use xilem::{
    AnyWidgetView, AppState, Color, EventLoop, TextAlign, WidgetView, WindowId,
    WindowOptionsExtLinux, WindowView, Xilem,
    core::{MessageProxy, fork},
    dpi::LogicalSize,
    masonry::{
        core::{DefaultProperties, PropertyStack, Selector},
        layout::AsUnit,
        parley::fontique::GenericFamily,
        properties::{
            Background, BorderColor, BorderWidth, CaretColor, ContentColor, CornerRadius, Gap,
            Padding, PlaceholderColor,
        },
        theme::default_property_set,
        widgets::{Button, Label, TextArea, TextInput},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexSpacer, button, flex_col, flex_row, label, sized_box, task_raw,
        text_button, text_input,
    },
    window,
};

use crate::{
    ShellCommand,
    app_catalog::{AppCatalog, DesktopApp},
};
use hearthspace_common::{
    CONTROL_BAR_HEIGHT, LAUNCHER_APP_ID, SHELL_BAR_APP_ID, SHELL_COMMAND_SOCKET_ENV,
    SHELL_OUTPUT_WATCHER_APP_ID, launcher_app_id_for_output, shell_bar_app_id_for_output,
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

const FALLBACK_OUTPUT_KEY: &str = "";

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
    properties.insert::<TextInput, _>(BorderColor {
        color: INPUT_BORDER,
    });
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
    outputs: Vec<ShellOutputInstance>,
    watcher_window_id: WindowId,
    running: bool,
}

#[derive(Debug)]
struct ShellOutputInstance {
    output_name: Option<String>,
    query: String,
    bar_window_id: WindowId,
    launcher_window_id: WindowId,
}

#[derive(Debug)]
struct OutputNamesChanged {
    names: Vec<String>,
}

impl AppState for ShellState {
    fn keep_running(&self) -> bool {
        self.running
    }
}

impl ShellState {
    fn new(command_socket: PathBuf, output_names: Vec<String>) -> Self {
        let mut state = Self {
            command_socket,
            catalog: AppCatalog::load(),
            outputs: Vec::new(),
            watcher_window_id: WindowId::next(),
            running: true,
        };
        state.apply_output_names(output_names);
        state
    }

    fn apply_output_names(&mut self, output_names: Vec<String>) {
        let targets = normalized_output_targets(output_names);
        let mut existing = std::mem::take(&mut self.outputs);
        let mut outputs = Vec::with_capacity(targets.len());

        for output_name in targets {
            if let Some(index) = existing
                .iter()
                .position(|instance| instance.output_name == output_name)
            {
                outputs.push(existing.remove(index));
            } else {
                outputs.push(ShellOutputInstance::new(output_name));
            }
        }

        self.outputs = outputs;
    }

    fn output_query_mut(&mut self, output_key: &str) -> Option<&mut String> {
        self.outputs
            .iter_mut()
            .find(|output| output.key() == output_key)
            .map(|output| &mut output.query)
    }
}

impl ShellOutputInstance {
    fn new(output_name: Option<String>) -> Self {
        Self {
            output_name,
            query: String::new(),
            bar_window_id: WindowId::next(),
            launcher_window_id: WindowId::next(),
        }
    }

    fn key(&self) -> &str {
        self.output_name.as_deref().unwrap_or(FALLBACK_OUTPUT_KEY)
    }

    fn bar_app_id(&self) -> String {
        self.output_name
            .as_deref()
            .map(shell_bar_app_id_for_output)
            .unwrap_or_else(|| SHELL_BAR_APP_ID.to_string())
    }

    fn launcher_app_id(&self) -> String {
        self.output_name
            .as_deref()
            .map(launcher_app_id_for_output)
            .unwrap_or_else(|| LAUNCHER_APP_ID.to_string())
    }
}

fn normalized_output_targets(output_names: Vec<String>) -> Vec<Option<String>> {
    let mut targets: Vec<Option<String>> = Vec::new();
    for name in output_names {
        if name.is_empty()
            || targets
                .iter()
                .any(|target| target.as_deref() == Some(&name))
        {
            continue;
        }
        targets.push(Some(name));
    }

    if targets.is_empty() {
        targets.push(None);
    }
    targets
}

/// Search results to show in the launcher dropdown. Returns nothing while the
/// search field is empty so the palette stays closed until the user types.
fn search_results(state: &ShellState, output_key: &str) -> Vec<DesktopApp> {
    let Some(output) = state
        .outputs
        .iter()
        .find(|output| output.key() == output_key)
    else {
        return Vec::new();
    };
    let query = output.query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    state.catalog.search(query, MAX_RESULTS)
}

/// The control bar: the search field plus the compositor control buttons.
fn bar_view(
    output: &ShellOutputInstance,
    include_output_watcher: bool,
) -> impl WidgetView<ShellState> + use<> {
    let output_key = output.key().to_string();
    let search_output_key = output_key.clone();
    let search = text_input(
        output.query.clone(),
        move |state: &mut ShellState, value: String| {
            if let Some(query) = state.output_query_mut(&output_key) {
                *query = value;
            }
        },
    )
    .placeholder("Search apps...")
    .font(GenericFamily::SystemUi)
    .on_enter(move |state: &mut ShellState, value: String| {
        let query = value.trim();
        if query.is_empty() {
            return;
        }
        if let Some(app) = state.catalog.search(query, 1).into_iter().next() {
            launch_app(&state.command_socket, &app.id);
            if let Some(query) = state.output_query_mut(&search_output_key) {
                query.clear();
            }
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
    let _ = include_output_watcher;
    flex_row((
        FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
        sized_box(search)
            .fixed_width(SEARCH_BOX_WIDTH.px())
            .fixed_height(SEARCH_BOX_HEIGHT.px()),
        command_buttons,
        FlexSpacer::Fixed(SHELL_BAR_H_PAD.px()),
    ))
}

fn output_watcher_view() -> impl WidgetView<ShellState> + use<> {
    sized_box(fork(label(""), output_watcher_task(true)))
        .fixed_width(1.0.px())
        .fixed_height(1.0.px())
}

/// The launcher palette: a vertical list of result buttons. Selecting one
/// launches the app and clears the search field, which closes the palette.
fn launcher_view(
    output_key: String,
    results: Vec<DesktopApp>,
) -> impl WidgetView<ShellState> + use<> {
    let buttons: Vec<Box<AnyWidgetView<ShellState>>> = results
        .into_iter()
        .map(|app| {
            let app_id = app.id;
            let output_key = output_key.clone();
            // A left-justified label fixed to the row's content width so the
            // button's (centered) child fills the row and the text sits flush
            // left. Buttons are flat until hovered (see `shell_properties`).
            button(
                sized_box(label(app.name).text_alignment(TextAlign::Start))
                    .fixed_width(LAUNCHER_ITEM_WIDTH.px()),
                move |state: &mut ShellState| {
                    launch_app(&state.command_socket, &app_id);
                    if let Some(query) = state.output_query_mut(&output_key) {
                        query.clear();
                    }
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
    let watcher = window(
        state.watcher_window_id,
        "Hearthspace Output Watcher",
        output_watcher_view(),
    )
    .with_options(|options| {
        options
            .with_name(SHELL_OUTPUT_WATCHER_APP_ID, SHELL_OUTPUT_WATCHER_APP_ID)
            .with_decorations(false)
            .with_resizable(false)
            .with_initial_inner_size(LogicalSize::new(1.0, 1.0))
            .on_close(|state: &mut ShellState| state.running = false)
    });
    let mut windows = vec![watcher];
    for (index, output) in state.outputs.iter().enumerate() {
        let bar_app_id = output.bar_app_id();
        let bar = window(
            output.bar_window_id,
            "Hearthspace Shell",
            bar_view(output, index == 0),
        )
        .with_options(move |options| {
            options
                // Tag the surface so the compositor renders it as chrome-less shell.
                .with_name(&bar_app_id, &bar_app_id)
                // No client-side titlebar; the compositor owns shell placement.
                .with_decorations(false)
                .with_resizable(false)
                .with_initial_inner_size(LogicalSize::new(760.0, f64::from(CONTROL_BAR_HEIGHT)))
                .on_close(|state: &mut ShellState| state.running = false)
        });
        windows.push(bar);

        let output_key = output.key().to_string();
        let results = search_results(state, &output_key);
        if !results.is_empty() {
            let launcher_app_id = output.launcher_app_id();
            let launcher_id = output.launcher_window_id;
            let size = launcher_size(results.len());
            let launcher = window(
                launcher_id,
                "Hearthspace Launcher",
                launcher_view(output_key, results),
            )
            .with_options(move |options| {
                options
                    .with_name(&launcher_app_id, &launcher_app_id)
                    .with_decorations(false)
                    .with_resizable(false)
                    // The initial size must stay constant across rebuilds (winit
                    // cannot change it afterwards), so the dropdown is grown and
                    // shrunk to its contents through the reactive min/max size.
                    .with_initial_inner_size(launcher_size(MAX_RESULTS))
                    .with_min_inner_size(size)
                    .with_max_inner_size(size)
            });
            windows.push(launcher);
        }
    }

    windows.into_iter()
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command_socket = env::var_os(SHELL_COMMAND_SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{SHELL_COMMAND_SOCKET_ENV} is not set"))?;
    info!(path = %command_socket.display(), "starting Xilem shell");

    let output_names = initial_wayland_output_names().unwrap_or_else(|error| {
        error!(%error, "failed to read initial Wayland outputs; using primary shell fallback");
        Vec::new()
    });
    let state = ShellState::new(command_socket, output_names);

    let app = Xilem::new(state, app_logic)
        .with_default_properties(shell_properties())
        .with_default_base_color(SHELL_BG);
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

fn output_watcher_task(
    enabled: bool,
) -> impl xilem::core::View<ShellState, (), xilem::ViewCtx, Element = xilem::core::NoElement> + use<>
{
    task_raw(
        move |proxy: MessageProxy<OutputNamesChanged>, _state: &mut ShellState| async move {
            if !enabled {
                return;
            }
            let result = xilem::tokio::task::spawn_blocking(move || watch_wayland_outputs(proxy))
                .await
                .unwrap_or_else(|error| Err(io::Error::other(error)));
            if let Err(error) = result {
                error!(%error, "Wayland output watcher stopped");
            }
        },
        |state: &mut ShellState, message: OutputNamesChanged| {
            state.apply_output_names(message.names);
        },
    )
}

fn initial_wayland_output_names() -> io::Result<Vec<String>> {
    let connection = Connection::connect_to_env().map_err(io::Error::other)?;
    let mut event_queue = connection.new_event_queue();
    let queue_handle = event_queue.handle();
    let _registry = connection.display().get_registry(&queue_handle, ());
    let mut watcher = OutputWatcher::default();
    event_queue
        .roundtrip(&mut watcher)
        .map_err(io::Error::other)?;
    Ok(watcher.output_names())
}

fn watch_wayland_outputs(proxy: MessageProxy<OutputNamesChanged>) -> io::Result<()> {
    let connection = Connection::connect_to_env().map_err(io::Error::other)?;
    let mut event_queue = connection.new_event_queue();
    let queue_handle = event_queue.handle();
    let _registry = connection.display().get_registry(&queue_handle, ());
    let mut watcher = OutputWatcher::default();
    event_queue
        .roundtrip(&mut watcher)
        .map_err(io::Error::other)?;
    send_output_names(&proxy, &watcher)?;

    loop {
        event_queue
            .blocking_dispatch(&mut watcher)
            .map_err(io::Error::other)?;
        if watcher.take_changed() {
            send_output_names(&proxy, &watcher)?;
        }
    }
}

fn send_output_names(
    proxy: &MessageProxy<OutputNamesChanged>,
    watcher: &OutputWatcher,
) -> io::Result<()> {
    proxy
        .message(OutputNamesChanged {
            names: watcher.output_names(),
        })
        .map_err(|error| io::Error::other(error.to_string()))
}

#[derive(Default)]
struct OutputWatcher {
    outputs: Vec<OutputState>,
    changed: bool,
}

struct OutputState {
    global_name: u32,
    _output: wl_output::WlOutput,
    name: Option<String>,
}

struct OutputGlobal {
    global_name: u32,
}

impl OutputWatcher {
    fn bind_output(
        &mut self,
        registry: &wl_registry::WlRegistry,
        global_name: u32,
        version: u32,
        queue_handle: &QueueHandle<Self>,
    ) {
        if self
            .outputs
            .iter()
            .any(|output| output.global_name == global_name)
        {
            return;
        }

        let output = registry.bind::<wl_output::WlOutput, _, _>(
            global_name,
            version.min(4),
            queue_handle,
            OutputGlobal { global_name },
        );
        self.outputs.push(OutputState {
            global_name,
            _output: output,
            name: None,
        });
    }

    fn output_mut(&mut self, global_name: u32) -> Option<&mut OutputState> {
        self.outputs
            .iter_mut()
            .find(|output| output.global_name == global_name)
    }

    fn remove_output(&mut self, global_name: u32) {
        let original_len = self.outputs.len();
        self.outputs
            .retain(|output| output.global_name != global_name);
        self.changed |= self.outputs.len() != original_len;
    }

    fn output_names(&self) -> Vec<String> {
        self.outputs
            .iter()
            .filter_map(|output| output.name.clone())
            .collect()
    }

    fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for OutputWatcher {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _connection: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_output" => {
                state.bind_output(registry, name, version, queue_handle);
            }
            wl_registry::Event::GlobalRemove { name } => state.remove_output(name),
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, OutputGlobal> for OutputWatcher {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        data: &OutputGlobal,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        let Some(output) = state.output_mut(data.global_name) else {
            return;
        };
        if let wl_output::Event::Name { name } = event
            && output.name.as_deref() != Some(&name)
        {
            output.name = Some(name);
            state.changed = true;
        }
    }
}

fn launch_app(command_socket: &PathBuf, app_id: &str) {
    send_command(
        command_socket,
        &ShellCommand::LaunchApp(app_id.to_string()).wire_name(),
    );
}

fn send_command(command_socket: &PathBuf, command: &str) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_targets_fall_back_to_primary_when_no_outputs_are_known() {
        assert_eq!(normalized_output_targets(Vec::new()), vec![None]);
    }

    #[test]
    fn output_targets_keep_order_and_drop_duplicates() {
        assert_eq!(
            normalized_output_targets(vec![
                "eDP-1".to_string(),
                "HDMI-A-1".to_string(),
                "eDP-1".to_string(),
                String::new(),
            ]),
            vec![Some("eDP-1".to_string()), Some("HDMI-A-1".to_string())]
        );
    }

    #[test]
    fn shell_state_preserves_window_ids_and_query_across_output_updates() {
        let mut state = ShellState::new(PathBuf::from("/tmp/socket"), vec!["eDP-1".to_string()]);
        state.outputs[0].query = "term".to_string();
        let bar_window_id = state.outputs[0].bar_window_id;

        state.apply_output_names(vec!["eDP-1".to_string(), "HDMI-A-1".to_string()]);

        assert_eq!(state.outputs[0].bar_window_id, bar_window_id);
        assert_eq!(state.outputs[0].query, "term");
        assert_eq!(state.outputs[1].output_name.as_deref(), Some("HDMI-A-1"));
    }
}
