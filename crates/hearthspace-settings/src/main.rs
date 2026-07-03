use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_output, wl_registry},
};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::accesskit::{Node, Role};
use xilem::masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    QueryCtx, RegisterCtx, TextEvent, Widget,
};
use xilem::masonry::imaging::Painter;
use xilem::masonry::kurbo::{Axis, Point, Rect, Size, Stroke};
use xilem::masonry::layout::{LenReq, Length};
use xilem::masonry::palette;
use xilem::peniko::Color;
use xilem::{Pod, ViewCtx};

use hearthspace_ipc::{
    MonitorConfig, Settings, client_get_settings, client_set_monitor_position,
    client_set_monitor_scale, client_set_test,
};
use xilem::{
    AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem,
    dpi::LogicalSize,
    masonry::{layout::AsUnit, parley::fontique::GenericFamily},
    style::Style,
    view::{checkbox, flex_col, flex_row, label, sized_box, text_button},
};

const EDITOR_PADDING: f64 = 24.0;
const MONITOR_SNAP: i32 = 10;
const MONITOR_LAYOUT_CANVAS_MULTIPLIER: f64 = 1.2;
const EMPTY_MONITOR_CANVAS_WIDTH: i32 = 1920;
const EMPTY_MONITOR_CANVAS_HEIGHT: i32 = 1080;
const MONITOR_SCALE_OPTIONS: [i32; 4] = [1, 2, 3, 4];

struct SettingsApp {
    settings: Settings,
    live_outputs: Vec<LiveOutput>,
    selected_monitor_id: Option<String>,
    status: String,
    config_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveOutput {
    id: String,
    name: String,
    width: i32,
    height: i32,
    scale: i32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = initial_state();
    let app = Xilem::new_simple(
        state,
        app_logic,
        WindowOptions::new("Hearthspace Settings")
            .with_initial_inner_size(LogicalSize::new(840.0, 720.0))
            .with_min_inner_size(LogicalSize::new(680.0, 600.0)),
    );

    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}

fn initial_state() -> SettingsApp {
    let (mut settings, mut status) = match client_get_settings() {
        Ok(settings) => (settings, "Connected to hearthspace-settingsd".to_string()),
        Err(error) => (
            Settings::default(),
            format!("Settings backend unavailable: {error}"),
        ),
    };
    let live_outputs = match live_wayland_outputs() {
        Ok(outputs) => outputs,
        Err(error) => {
            status.push_str(&format!("; live monitors unavailable: {error}"));
            Vec::new()
        }
    };
    settings = settings_with_live_outputs(settings, &live_outputs);
    let selected_monitor_id = settings.monitors.first().map(|monitor| monitor.id.clone());

    let config_label = match hearthspace_common::config_path() {
        Ok(path) => format!("Config: {}", path.display()),
        Err(error) => format!("Config path unavailable: {error}"),
    };

    SettingsApp {
        settings,
        live_outputs,
        selected_monitor_id,
        status,
        config_label,
    }
}

fn app_logic(state: &mut SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let test_checked = state.settings.test;
    let monitor_summary = format_monitor_summary(&state.settings.monitors);

    sized_box(
        flex_col((
            label("Hearthspace Settings")
                .font(GenericFamily::SystemUi)
                .text_size(22.0),
            label("Drag monitors to arrange their relative positions."),
            sized_box(monitor_layout_editor()).fixed_height(320.0.px()),
            label(monitor_summary),
            monitor_details_view(state),
            checkbox("test", test_checked, |state: &mut SettingsApp, checked| {
                let previous = state.settings.test;
                state.settings.test = checked;

                match client_set_test(checked) {
                    Ok(settings) => {
                        let saved_test = settings.test;
                        state.settings = settings_with_live_outputs(settings, &state.live_outputs);
                        state.status = format!("Saved test = {saved_test}");
                    }
                    Err(error) => {
                        state.settings.test = previous;
                        state.status = format!("Failed to save setting: {error}");
                    }
                }
            }),
            label(state.config_label.clone()),
            label(state.status.clone()),
        ))
        .gap(12.0.px()),
    )
    .padding(20.0.px())
}

fn format_monitor_summary(monitors: &[MonitorConfig]) -> String {
    monitors
        .iter()
        .map(|monitor| {
            format!(
                "{}: {}x{} @ {}x at {},{}",
                monitor.name, monitor.width, monitor.height, monitor.scale, monitor.x, monitor.y
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn monitor_details_view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let Some(monitor) = selected_monitor(state) else {
        return label("Select a monitor to edit its settings.").boxed();
    };

    let scale_buttons = MONITOR_SCALE_OPTIONS.map(|scale| {
        let id = monitor.id.clone();
        text_button(format!("{}x", scale), move |state: &mut SettingsApp| {
            save_monitor_scale(state, &id, scale);
        })
    });

    flex_col((
        label(format!("Selected: {}", monitor.name)),
        label(format!(
            "Physical: {}x{} | Desktop: {}x{} | Position: {},{}",
            monitor.width,
            monitor.height,
            monitor_logical_width(monitor),
            monitor_logical_height(monitor),
            monitor.x,
            monitor.y
        )),
        flex_row((label(format!("Scale: {}x", monitor.scale)), scale_buttons)),
    ))
    .gap(8.0.px())
    .boxed()
}

fn selected_monitor(state: &SettingsApp) -> Option<&MonitorConfig> {
    let selected_id = state.selected_monitor_id.as_ref()?;
    state
        .settings
        .monitors
        .iter()
        .find(|monitor| &monitor.id == selected_id)
}

fn monitor_layout_editor() -> MonitorLayoutEditor {
    MonitorLayoutEditor
}

struct MonitorLayoutEditor;

impl ViewMarker for MonitorLayoutEditor {}

impl View<SettingsApp, (), ViewCtx> for MonitorLayoutEditor {
    type Element = Pod<MonitorLayoutWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        (
            ctx.with_action_widget(|ctx| {
                ctx.create_pod(MonitorLayoutWidget::new(
                    state.settings.monitors.clone(),
                    state.selected_monitor_id.clone(),
                ))
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) {
        element.widget.set_monitors(
            state.settings.monitors.clone(),
            state.selected_monitor_id.clone(),
            &mut element.ctx,
        );
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        let Some(action) = message.take_message::<MonitorEditorAction>() else {
            return MessageResult::Stale;
        };
        apply_monitor_editor_action(state, *action);
        MessageResult::Action(())
    }
}

fn apply_monitor_editor_action(state: &mut SettingsApp, action: MonitorEditorAction) {
    match action {
        MonitorEditorAction::Select { id, monitors } => {
            state.settings.monitors = monitors;
            state.selected_monitor_id = Some(id.clone());
            state.status = format!("Selected monitor {id}");
        }
        MonitorEditorAction::Drag {
            moved_id,
            monitors,
            commit,
        } => {
            let moved_monitor = monitors
                .iter()
                .find(|monitor| monitor.id == moved_id)
                .cloned();
            state.selected_monitor_id = Some(moved_id.clone());
            state.settings.monitors = monitors;

            if commit {
                match save_monitor_positions(&state.settings.monitors) {
                    Ok(Some(settings)) => {
                        state.settings = settings_with_live_outputs(settings, &state.live_outputs);
                        keep_selected_monitor_available(state);
                        state.status = format!("Saved monitor layout for {moved_id}");
                    }
                    Ok(None) => state.status = "Saved empty monitor layout".to_string(),
                    Err(error) => state.status = format!("Failed to save monitor layout: {error}"),
                }
            } else if let Some(monitor) = moved_monitor {
                state.status = format!("Moving {} to {},{}", monitor.name, monitor.x, monitor.y);
            } else {
                state.status = format!("Unknown monitor: {moved_id}");
            }
        }
    }
}

fn keep_selected_monitor_available(state: &mut SettingsApp) {
    if state.selected_monitor_id.as_ref().is_some_and(|id| {
        state
            .settings
            .monitors
            .iter()
            .any(|monitor| &monitor.id == id)
    }) {
        return;
    }
    state.selected_monitor_id = state
        .settings
        .monitors
        .first()
        .map(|monitor| monitor.id.clone());
}

fn save_monitor_scale(state: &mut SettingsApp, id: &str, scale: i32) {
    let previous = state.settings.clone();
    let mut monitors = state.settings.monitors.clone();
    recalculate_monitor_positions_for_scale(&mut monitors, id, scale);
    state.settings.monitors = monitors.clone();

    match client_set_monitor_scale(id, scale) {
        Ok(_) => match save_monitor_positions(&monitors) {
            Ok(Some(settings)) => {
                state.settings = settings_with_live_outputs(settings, &state.live_outputs);
                state.selected_monitor_id = Some(id.to_string());
                keep_selected_monitor_available(state);
                state.status = format!("Saved monitor scale for {id}: {scale}x");
            }
            Ok(None) => {
                state.settings.monitors = monitors;
                state.selected_monitor_id = Some(id.to_string());
                state.status = format!("Saved monitor scale for {id}: {scale}x");
            }
            Err(error) => {
                let _ = previous
                    .monitors
                    .iter()
                    .find(|monitor| monitor.id == id)
                    .map(|monitor| client_set_monitor_scale(id, monitor.scale));
                state.settings = previous;
                state.status = format!("Failed to save monitor positions after scale: {error}");
            }
        },
        Err(error) => {
            state.settings = previous;
            state.status = format!("Failed to save monitor scale: {error}");
        }
    }
}

fn recalculate_monitor_positions_for_scale(monitors: &mut [MonitorConfig], id: &str, scale: i32) {
    let Some(index) = monitors.iter().position(|monitor| monitor.id == id) else {
        return;
    };
    let old_monitor = monitors[index].clone();
    let old_width = monitor_logical_width(&old_monitor);
    let old_height = monitor_logical_height(&old_monitor);
    let old_right = old_monitor.x + old_width;
    let old_bottom = old_monitor.y + old_height;

    monitors[index].scale = scale.max(1);
    let new_width = monitor_logical_width(&monitors[index]);
    let new_height = monitor_logical_height(&monitors[index]);
    let dx = new_width - old_width;
    let dy = new_height - old_height;

    for (monitor_index, monitor) in monitors.iter_mut().enumerate() {
        if monitor_index == index {
            continue;
        }
        if monitor.x >= old_right {
            monitor.x += dx;
        }
        if monitor.y >= old_bottom {
            monitor.y += dy;
        }
    }

    if monitors_overlap(monitors) {
        place_monitors_edge_to_edge(monitors);
    }
}

fn settings_with_live_outputs(mut settings: Settings, live_outputs: &[LiveOutput]) -> Settings {
    if live_outputs.is_empty() {
        return settings;
    }

    settings.monitors = monitors_for_live_outputs(&settings.monitors, live_outputs);
    settings
}

fn monitors_for_live_outputs(
    saved_monitors: &[MonitorConfig],
    live_outputs: &[LiveOutput],
) -> Vec<MonitorConfig> {
    let mut monitors = Vec::with_capacity(live_outputs.len());
    let mut next_x = 0;
    let saved_monitors_are_placeholders = default_placeholder_monitors(saved_monitors);
    for (index, output) in live_outputs.iter().enumerate() {
        let role_id = monitor_role_id(index);
        let saved = (!saved_monitors_are_placeholders)
            .then(|| {
                saved_monitors
                    .iter()
                    .find(|monitor| monitor.id == output.id || monitor.name == output.name)
                    .or_else(|| saved_monitors.iter().find(|monitor| monitor.id == role_id))
            })
            .flatten();
        let (id, scale, x, y) = if let Some(saved) = saved {
            (saved.id.clone(), saved.scale.max(1), saved.x, saved.y)
        } else if saved_monitors_are_placeholders && index < 2 {
            (role_id, output.scale.max(1), next_x, 0)
        } else {
            (output.id.clone(), output.scale.max(1), next_x, 0)
        };
        monitors.push(MonitorConfig {
            id,
            name: output.name.clone(),
            width: output.width,
            height: output.height,
            scale,
            x,
            y,
        });
        next_x += logical_dimension(output.width, scale);
    }
    if monitors_overlap(&monitors) {
        place_monitors_edge_to_edge(&mut monitors);
    }
    monitors
}

fn place_monitors_edge_to_edge(monitors: &mut [MonitorConfig]) {
    let mut next_x = 0;
    for monitor in monitors {
        monitor.x = next_x;
        monitor.y = 0;
        next_x += monitor_logical_width(monitor);
    }
}

fn monitors_overlap(monitors: &[MonitorConfig]) -> bool {
    monitors.iter().enumerate().any(|(index, monitor)| {
        monitors
            .iter()
            .skip(index + 1)
            .any(|other| monitor_rects_overlap(monitor, other))
    })
}

fn monitor_rects_overlap(a: &MonitorConfig, b: &MonitorConfig) -> bool {
    a.x < b.x + monitor_logical_width(b)
        && a.x + monitor_logical_width(a) > b.x
        && a.y < b.y + monitor_logical_height(b)
        && a.y + monitor_logical_height(a) > b.y
}

fn monitor_logical_width(monitor: &MonitorConfig) -> i32 {
    logical_dimension(monitor.width, monitor.scale)
}

fn monitor_logical_height(monitor: &MonitorConfig) -> i32 {
    logical_dimension(monitor.height, monitor.scale)
}

fn logical_dimension(size: i32, _scale: i32) -> i32 {
    size.max(1)
}

fn default_placeholder_monitors(monitors: &[MonitorConfig]) -> bool {
    monitors == Settings::default().monitors
}

fn monitor_role_id(index: usize) -> String {
    if index == 0 {
        "primary".to_string()
    } else if index == 1 {
        "secondary".to_string()
    } else {
        format!("secondary-{index}")
    }
}

fn live_wayland_outputs() -> std::io::Result<Vec<LiveOutput>> {
    let connection = Connection::connect_to_env().map_err(std::io::Error::other)?;
    let (globals, mut event_queue) =
        registry_queue_init::<OutputProbe>(&connection).map_err(std::io::Error::other)?;
    let queue_handle = event_queue.handle();
    let mut probe = OutputProbe::default();
    for global in globals.contents().clone_list() {
        if global.interface == "wl_output" {
            globals.registry().bind::<wl_output::WlOutput, _, _>(
                global.name,
                global.version.min(4),
                &queue_handle,
                OutputGlobal { name: global.name },
            );
        }
    }
    event_queue
        .roundtrip(&mut probe)
        .map_err(std::io::Error::other)?;
    Ok(probe.live_outputs())
}

#[derive(Default)]
struct OutputProbe {
    outputs: Vec<OutputState>,
}

struct OutputGlobal {
    name: u32,
}

#[derive(Debug)]
struct OutputState {
    global_name: u32,
    name: Option<String>,
    description: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    scale: Option<i32>,
}

impl OutputProbe {
    fn output_mut(&mut self, global_name: u32) -> &mut OutputState {
        if let Some(index) = self
            .outputs
            .iter()
            .position(|output| output.global_name == global_name)
        {
            return &mut self.outputs[index];
        }
        self.outputs.push(OutputState {
            global_name,
            name: None,
            description: None,
            width: None,
            height: None,
            scale: None,
        });
        self.outputs.last_mut().unwrap()
    }

    fn live_outputs(self) -> Vec<LiveOutput> {
        self.outputs
            .into_iter()
            .map(|output| {
                let fallback = format!("output-{}", output.global_name);
                let id = output.name.clone().unwrap_or_else(|| fallback.clone());
                LiveOutput {
                    id: id.clone(),
                    name: output.description.or(output.name).unwrap_or(id),
                    width: output.width.unwrap_or(EMPTY_MONITOR_CANVAS_WIDTH),
                    height: output.height.unwrap_or(EMPTY_MONITOR_CANVAS_HEIGHT),
                    scale: output.scale.unwrap_or(1).max(1),
                }
            })
            .collect()
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for OutputProbe {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, OutputGlobal> for OutputProbe {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        data: &OutputGlobal,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        let output = state.output_mut(data.name);
        match event {
            wl_output::Event::Name { name } => output.name = Some(name),
            wl_output::Event::Description { description } => {
                output.description = Some(description);
            }
            wl_output::Event::Mode {
                flags,
                width,
                height,
                ..
            } if output_size_is_current(flags) => {
                output.width = Some(width);
                output.height = Some(height);
            }
            wl_output::Event::Scale { factor } => output.scale = Some(factor.max(1)),
            _ => {}
        }
    }
}

fn output_size_is_current(flags: WEnum<wl_output::Mode>) -> bool {
    matches!(flags, WEnum::Value(flags) if flags.contains(wl_output::Mode::Current))
}

fn save_monitor_positions(monitors: &[MonitorConfig]) -> std::io::Result<Option<Settings>> {
    let mut saved = None;
    for monitor in monitors {
        saved = Some(client_set_monitor_position(
            &monitor.id,
            monitor.x,
            monitor.y,
        )?);
    }
    Ok(saved)
}

#[derive(Debug)]
enum MonitorEditorAction {
    Select {
        id: String,
        monitors: Vec<MonitorConfig>,
    },
    Drag {
        moved_id: String,
        monitors: Vec<MonitorConfig>,
        commit: bool,
    },
}

const DRAG_START_DISTANCE: f64 = 3.0;

#[derive(Debug)]
struct MonitorLayoutWidget {
    monitors: Vec<MonitorConfig>,
    selected_monitor_id: Option<String>,
    drag: Option<MonitorDrag>,
}

#[derive(Debug, Clone)]
struct MonitorDrag {
    id: String,
    start_pointer: Point,
    start_x: i32,
    start_y: i32,
    origin_x: f64,
    origin_y: f64,
    scale: f64,
    moved: bool,
}

impl MonitorLayoutWidget {
    fn new(monitors: Vec<MonitorConfig>, selected_monitor_id: Option<String>) -> Self {
        Self {
            monitors,
            selected_monitor_id,
            drag: None,
        }
    }

    fn set_monitors(
        &mut self,
        monitors: Vec<MonitorConfig>,
        selected_monitor_id: Option<String>,
        ctx: &mut xilem::masonry::core::MutateCtx<'_>,
    ) {
        if self.monitors != monitors || self.selected_monitor_id != selected_monitor_id {
            self.monitors = monitors;
            self.selected_monitor_id = selected_monitor_id;
            ctx.request_render();
        }
    }

    fn layout_metrics(&self, size: Size) -> LayoutMetrics {
        if let Some(drag) = &self.drag {
            return LayoutMetrics {
                origin_x: drag.origin_x,
                origin_y: drag.origin_y,
                scale: drag.scale,
            };
        }

        let canvas_size = monitor_canvas_size(&self.monitors);
        let scale = layout_scale_for_canvas(&self.monitors, size);
        let (origin_x, origin_y) = monitor_canvas_origin(&self.monitors, canvas_size);

        LayoutMetrics {
            origin_x,
            origin_y,
            scale,
        }
    }

    fn monitor_rect(&self, monitor: &MonitorConfig, metrics: LayoutMetrics) -> Rect {
        Rect::new(
            EDITOR_PADDING + (f64::from(monitor.x) - metrics.origin_x) * metrics.scale,
            EDITOR_PADDING + (f64::from(monitor.y) - metrics.origin_y) * metrics.scale,
            EDITOR_PADDING
                + (f64::from(monitor.x + monitor_logical_width(monitor)) - metrics.origin_x)
                    * metrics.scale,
            EDITOR_PADDING
                + (f64::from(monitor.y + monitor_logical_height(monitor)) - metrics.origin_y)
                    * metrics.scale,
        )
    }

    fn hit_monitor(&self, point: Point, metrics: LayoutMetrics) -> Option<&MonitorConfig> {
        self.monitors
            .iter()
            .rev()
            .find(|monitor| self.monitor_rect(monitor, metrics).contains(point))
    }

    fn monitors_for_drag(
        &self,
        drag: &MonitorDrag,
        point: Point,
        normalize: bool,
    ) -> Vec<MonitorConfig> {
        let x = drag.start_x + ((point.x - drag.start_pointer.x) / drag.scale).round() as i32;
        let y = drag.start_y + ((point.y - drag.start_pointer.y) / drag.scale).round() as i32;
        let (x, y) =
            snap_monitor_to_neighbor(&self.monitors, &drag.id, snap_position(x), snap_position(y));
        let mut monitors = self.monitors.clone();
        set_monitor_position(&mut monitors, &drag.id, x, y);
        if normalize {
            normalize_monitors_near_origin(&mut monitors);
        }
        monitors
    }
}

impl Widget for MonitorLayoutWidget {
    type Action = MonitorEditorAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let point = ctx.local_position(state.position);
                let metrics = self.layout_metrics(ctx.content_box().size());
                if let Some(monitor) = self.hit_monitor(point, metrics) {
                    let selected_id = monitor.id.clone();
                    let start_x = monitor.x;
                    let start_y = monitor.y;
                    ctx.capture_pointer();
                    ctx.request_focus();
                    ctx.set_handled();
                    self.selected_monitor_id = Some(selected_id.clone());
                    self.drag = Some(MonitorDrag {
                        id: selected_id.clone(),
                        start_pointer: point,
                        start_x,
                        start_y,
                        origin_x: metrics.origin_x,
                        origin_y: metrics.origin_y,
                        scale: metrics.scale,
                        moved: false,
                    });
                    ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Select {
                        id: selected_id,
                        monitors: self.monitors.clone(),
                    });
                    ctx.request_render();
                }
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                if let Some(drag) = &mut self.drag {
                    let point = ctx.local_position(current.position);
                    if !drag.moved && point.distance(drag.start_pointer) < DRAG_START_DISTANCE {
                        return;
                    }
                    drag.moved = true;
                    let drag = drag.clone();
                    let moved_id = drag.id.clone();
                    let monitors = self.monitors_for_drag(&drag, point, false);
                    self.monitors = monitors.clone();
                    ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Drag {
                        moved_id,
                        monitors,
                        commit: false,
                    });
                    ctx.request_render();
                    ctx.set_handled();
                }
            }
            PointerEvent::Up(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                if let Some(drag) = self.drag.take() {
                    if !drag.moved {
                        self.selected_monitor_id = Some(drag.id.clone());
                        ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Select {
                            id: drag.id,
                            monitors: self.monitors.clone(),
                        });
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    let point = ctx.local_position(state.position);
                    let monitors = self.monitors_for_drag(&drag, point, true);
                    self.monitors = monitors.clone();
                    ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Drag {
                        moved_id: drag.id,
                        monitors,
                        commit: true,
                    });
                    ctx.request_render();
                    ctx.set_handled();
                }
            }
            PointerEvent::Cancel(..) => self.drag = None,
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Horizontal => 520.0.px(),
            Axis::Vertical => 300.0.px(),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let content = ctx.content_box();
        painter
            .fill(content, Color::from_rgb8(0x13, 0x17, 0x22))
            .draw();

        let metrics = self.layout_metrics(content.size());
        for monitor in &self.monitors {
            let rect = self.monitor_rect(monitor, metrics);
            let active = self.drag.as_ref().is_some_and(|drag| drag.id == monitor.id);
            let selected = self
                .selected_monitor_id
                .as_ref()
                .is_some_and(|id| id == &monitor.id);
            let fill = if active {
                Color::from_rgb8(0x74, 0x9C, 0xFF)
            } else if selected {
                Color::from_rgb8(0x50, 0x72, 0xD8)
            } else {
                Color::from_rgb8(0x38, 0x55, 0xA3)
            };
            painter.fill(rect, fill).draw();
            painter
                .stroke(rect, &Stroke::new(2.0), palette::css::WHITE)
                .draw();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label("Drag monitor rectangles to configure their arrangement");
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        let metrics = self.layout_metrics(ctx.content_box().size());
        if self.hit_monitor(ctx.to_local(pos), metrics).is_some() {
            CursorIcon::Grab
        } else {
            CursorIcon::Default
        }
    }
}

#[derive(Clone, Copy)]
struct LayoutMetrics {
    origin_x: f64,
    origin_y: f64,
    scale: f64,
}

fn monitor_canvas_size(monitors: &[MonitorConfig]) -> Size {
    let (width, height) = monitor_canvas_span(monitors);
    Size::new(
        f64::from(width) * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
        f64::from(height) * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
    )
}

fn monitor_canvas_origin(monitors: &[MonitorConfig], canvas_size: Size) -> (f64, f64) {
    let Some((min_x, min_y, max_x, max_y)) = monitor_arrangement_bounds(monitors) else {
        return (canvas_size.width / -2.0, canvas_size.height / -2.0);
    };
    let center_x = (f64::from(min_x) + f64::from(max_x)) / 2.0;
    let center_y = (f64::from(min_y) + f64::from(max_y)) / 2.0;
    (
        center_x - canvas_size.width / 2.0,
        center_y - canvas_size.height / 2.0,
    )
}

fn monitor_canvas_span(monitors: &[MonitorConfig]) -> (i32, i32) {
    if monitors.is_empty() {
        return (EMPTY_MONITOR_CANVAS_WIDTH, EMPTY_MONITOR_CANVAS_HEIGHT);
    }

    (
        combined_monitor_width(monitors),
        combined_monitor_height(monitors),
    )
}

fn combined_monitor_width(monitors: &[MonitorConfig]) -> i32 {
    monitors.iter().map(monitor_logical_width).sum()
}

fn combined_monitor_height(monitors: &[MonitorConfig]) -> i32 {
    monitors.iter().map(monitor_logical_height).sum()
}

fn layout_scale_for_canvas(monitors: &[MonitorConfig], size: Size) -> f64 {
    let canvas_size = monitor_canvas_size(monitors);
    let available_width = (size.width - EDITOR_PADDING * 2.0).max(1.0);
    let available_height = (size.height - EDITOR_PADDING * 2.0).max(1.0);
    (available_width / canvas_size.width)
        .min(available_height / canvas_size.height)
        .clamp(0.03, 1.0)
}

fn set_monitor_position(monitors: &mut [MonitorConfig], id: &str, x: i32, y: i32) {
    if let Some(monitor) = monitors.iter_mut().find(|monitor| monitor.id == id) {
        monitor.x = x;
        monitor.y = y;
    }
}

fn normalize_monitors_near_origin(monitors: &mut [MonitorConfig]) {
    let Some((min_x, min_y, max_x, max_y)) = monitor_arrangement_bounds(monitors) else {
        return;
    };
    let center_x = midpoint_i32(min_x, max_x);
    let center_y = midpoint_i32(min_y, max_y);
    for monitor in monitors {
        monitor.x -= center_x;
        monitor.y -= center_y;
    }
}

fn monitor_arrangement_bounds(monitors: &[MonitorConfig]) -> Option<(i32, i32, i32, i32)> {
    let first = monitors.first()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + monitor_logical_width(first);
    let mut max_y = first.y + monitor_logical_height(first);
    for monitor in monitors.iter().skip(1) {
        min_x = min_x.min(monitor.x);
        min_y = min_y.min(monitor.y);
        max_x = max_x.max(monitor.x + monitor_logical_width(monitor));
        max_y = max_y.max(monitor.y + monitor_logical_height(monitor));
    }
    Some((min_x, min_y, max_x, max_y))
}

fn midpoint_i32(start: i32, end: i32) -> i32 {
    ((i64::from(start) + i64::from(end)) / 2) as i32
}

#[cfg(test)]
fn offset_monitor(monitors: &mut [MonitorConfig], id: &str, x: i32, y: i32) {
    set_monitor_position(monitors, id, x, y);
}

fn snap_position(value: i32) -> i32 {
    let half = MONITOR_SNAP / 2;
    if value >= 0 {
        ((value + half) / MONITOR_SNAP) * MONITOR_SNAP
    } else {
        ((value - half) / MONITOR_SNAP) * MONITOR_SNAP
    }
}

fn snap_monitor_to_neighbor(monitors: &[MonitorConfig], id: &str, x: i32, y: i32) -> (i32, i32) {
    let Some(monitor) = monitors.iter().find(|monitor| monitor.id == id) else {
        return (x, y);
    };
    let candidates = monitors
        .iter()
        .filter(|other| other.id != id)
        .flat_map(|other| bordered_positions_for(monitor, other, x, y));

    candidates
        .min_by_key(|(candidate_x, candidate_y)| {
            let dx = i64::from(*candidate_x - x);
            let dy = i64::from(*candidate_y - y);
            dx * dx + dy * dy
        })
        .unwrap_or((x, y))
}

fn bordered_positions_for(
    monitor: &MonitorConfig,
    other: &MonitorConfig,
    x: i32,
    y: i32,
) -> [(i32, i32); 4] {
    let monitor_width = monitor_logical_width(monitor);
    let monitor_height = monitor_logical_height(monitor);
    let other_width = monitor_logical_width(other);
    let other_height = monitor_logical_height(other);
    let left_of_other = other.x - monitor_width;
    let right_of_other = other.x + other_width;
    let above_other = other.y - monitor_height;
    let below_other = other.y + other_height;

    [
        (
            left_of_other,
            clamp_overlap(y, monitor_height, other.y, other_height),
        ),
        (
            right_of_other,
            clamp_overlap(y, monitor_height, other.y, other_height),
        ),
        (
            clamp_overlap(x, monitor_width, other.x, other_width),
            above_other,
        ),
        (
            clamp_overlap(x, monitor_width, other.x, other_width),
            below_other,
        ),
    ]
}

fn clamp_overlap(position: i32, size: i32, other_position: i32, other_size: i32) -> i32 {
    let size = size.max(1);
    let other_size = other_size.max(1);
    position.clamp(other_position - size + 1, other_position + other_size - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_canvas_size_uses_combined_dimensions_with_multiplier() {
        let monitors = vec![
            MonitorConfig {
                id: "left".to_string(),
                name: "Left".to_string(),
                width: 1280,
                height: 720,
                scale: 1,
                x: -1280,
                y: 120,
            },
            MonitorConfig {
                id: "main".to_string(),
                name: "Main".to_string(),
                width: 1920,
                height: 1080,
                scale: 1,
                x: 0,
                y: 0,
            },
        ];

        assert_eq!(combined_monitor_width(&monitors), 3200);
        assert_eq!(combined_monitor_height(&monitors), 1800);
        assert_eq!(
            monitor_canvas_size(&monitors),
            Size::new(
                3200.0 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
                1800.0 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
            )
        );
    }

    #[test]
    fn monitor_canvas_size_does_not_cap_large_monitors() {
        let monitors = vec![
            MonitorConfig {
                id: "ultrawide".to_string(),
                name: "Ultrawide".to_string(),
                width: 5120,
                height: 1440,
                scale: 1,
                x: 0,
                y: 0,
            },
            MonitorConfig {
                id: "hidpi".to_string(),
                name: "HiDPI".to_string(),
                width: 3840,
                height: 2160,
                scale: 1,
                x: 5120,
                y: 0,
            },
        ];

        assert_eq!(combined_monitor_width(&monitors), 8960);
        assert_eq!(combined_monitor_height(&monitors), 3600);
        assert_eq!(
            monitor_canvas_size(&monitors),
            Size::new(
                8960.0 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
                3600.0 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
            )
        );
    }

    #[test]
    fn monitor_canvas_size_uses_default_only_when_no_monitors_exist() {
        assert_eq!(
            monitor_canvas_size(&[]),
            Size::new(
                f64::from(EMPTY_MONITOR_CANVAS_WIDTH) * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
                f64::from(EMPTY_MONITOR_CANVAS_HEIGHT) * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
            )
        );
    }

    #[test]
    fn live_outputs_hide_saved_disconnected_monitors() {
        let monitors = monitors_for_live_outputs(
            &Settings::default().monitors,
            &[LiveOutput {
                id: "eDP-1".to_string(),
                name: "Built-in Display".to_string(),
                width: 2256,
                height: 1504,
                scale: 1,
            }],
        );

        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].id, "primary");
        assert_eq!(monitors[0].name, "Built-in Display");
        assert_eq!((monitors[0].width, monitors[0].height), (2256, 1504));
    }

    #[test]
    fn live_outputs_do_not_reuse_placeholder_positions_for_mixed_resolutions() {
        let monitors = monitors_for_live_outputs(
            &Settings::default().monitors,
            &[
                LiveOutput {
                    id: "eDP-1".to_string(),
                    name: "Built-in Display".to_string(),
                    width: 2560,
                    height: 1440,
                    scale: 1,
                },
                LiveOutput {
                    id: "HDMI-A-1".to_string(),
                    name: "External Monitor".to_string(),
                    width: 1366,
                    height: 768,
                    scale: 1,
                },
            ],
        );

        assert_eq!(monitors[0].id, "primary");
        assert_eq!((monitors[0].x, monitors[0].y), (0, 0));
        assert_eq!(monitors[1].id, "secondary");
        assert_eq!((monitors[1].x, monitors[1].y), (2560, 0));
    }

    #[test]
    fn live_outputs_repair_overlapping_saved_active_monitors() {
        let saved = vec![
            MonitorConfig {
                id: "primary".to_string(),
                name: "Primary".to_string(),
                width: 1920,
                height: 1080,
                scale: 1,
                x: -1280,
                y: -736,
            },
            MonitorConfig {
                id: "secondary".to_string(),
                name: "Secondary".to_string(),
                width: 1920,
                height: 1080,
                scale: 1,
                x: -1920,
                y: -540,
            },
        ];

        let monitors = monitors_for_live_outputs(
            &saved,
            &[
                LiveOutput {
                    id: "eDP-1".to_string(),
                    name: "Built-in Display".to_string(),
                    width: 2560,
                    height: 1440,
                    scale: 1,
                },
                LiveOutput {
                    id: "HDMI-A-1".to_string(),
                    name: "External Monitor".to_string(),
                    width: 1366,
                    height: 768,
                    scale: 1,
                },
            ],
        );

        assert_eq!((monitors[0].x, monitors[0].y), (0, 0));
        assert_eq!((monitors[1].x, monitors[1].y), (2560, 0));
        assert!(!monitors_overlap(&monitors));
    }

    #[test]
    fn live_outputs_preserve_saved_connector_positions() {
        let saved = vec![MonitorConfig {
            id: "HDMI-A-1".to_string(),
            name: "HDMI-A-1".to_string(),
            width: 1920,
            height: 1080,
            scale: 2,
            x: -1920,
            y: 200,
        }];

        let monitors = monitors_for_live_outputs(
            &saved,
            &[LiveOutput {
                id: "HDMI-A-1".to_string(),
                name: "External Monitor".to_string(),
                width: 2560,
                height: 1440,
                scale: 1,
            }],
        );

        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].id, "HDMI-A-1");
        assert_eq!(monitors[0].name, "External Monitor");
        assert_eq!((monitors[0].x, monitors[0].y), (-1920, 200));
        assert_eq!((monitors[0].width, monitors[0].height), (2560, 1440));
        assert_eq!(monitors[0].scale, 2);
    }

    #[test]
    fn live_outputs_use_reported_scale_for_new_monitors() {
        let monitors = monitors_for_live_outputs(
            &[],
            &[LiveOutput {
                id: "eDP-1".to_string(),
                name: "Built-in Display".to_string(),
                width: 2560,
                height: 1440,
                scale: 2,
            }],
        );

        assert_eq!(monitors[0].scale, 2);
        assert_eq!(monitor_logical_width(&monitors[0]), 2560);
    }

    #[test]
    fn scaling_monitor_keeps_outputs_at_physical_desktop_size() {
        let mut monitors = Settings::default().monitors;

        recalculate_monitor_positions_for_scale(&mut monitors, "primary", 2);

        let primary = monitors
            .iter()
            .find(|monitor| monitor.id == "primary")
            .unwrap();
        let secondary = monitors
            .iter()
            .find(|monitor| monitor.id == "secondary")
            .unwrap();
        assert_eq!(primary.scale, 2);
        assert_eq!(monitor_logical_width(primary), 1920);
        assert_eq!((secondary.x, secondary.y), (1920, 0));
        assert!(!monitors_overlap(&monitors));
    }

    #[test]
    fn scaling_monitor_keeps_outputs_below_at_physical_desktop_size() {
        let mut monitors = Settings::default().monitors;
        offset_monitor(&mut monitors, "secondary", 0, 1080);

        recalculate_monitor_positions_for_scale(&mut monitors, "primary", 2);

        let secondary = monitors
            .iter()
            .find(|monitor| monitor.id == "secondary")
            .unwrap();
        assert_eq!((secondary.x, secondary.y), (0, 1080));
        assert!(!monitors_overlap(&monitors));
    }

    #[test]
    fn layout_scale_does_not_change_when_monitors_are_dragged() {
        let mut monitors = Settings::default().monitors;
        let size = Size::new(760.0, 320.0);
        let scale = layout_scale_for_canvas(&monitors, size);

        offset_monitor(&mut monitors, "secondary", -3200, 900);

        assert_eq!(layout_scale_for_canvas(&monitors, size), scale);
    }

    #[test]
    fn layout_metrics_do_not_recenter_during_live_drag() {
        let mut widget = MonitorLayoutWidget::new(Settings::default().monitors, None);
        let size = Size::new(760.0, 320.0);
        let metrics = widget.layout_metrics(size);
        widget.drag = Some(MonitorDrag {
            id: "secondary".to_string(),
            start_pointer: Point::from((0.0, 0.0)),
            start_x: 1920,
            start_y: 0,
            origin_x: metrics.origin_x,
            origin_y: metrics.origin_y,
            scale: metrics.scale,
            moved: false,
        });

        offset_monitor(&mut widget.monitors, "secondary", 0, 1200);

        let live_metrics = widget.layout_metrics(size);
        assert_eq!(live_metrics.origin_x, metrics.origin_x);
        assert_eq!(live_metrics.origin_y, metrics.origin_y);
        assert_eq!(live_metrics.scale, metrics.scale);
    }

    #[test]
    fn canvas_origin_centers_far_away_arrangements() {
        let mut monitors = Settings::default().monitors;
        offset_monitor(&mut monitors, "primary", 10_000, -8_000);
        offset_monitor(&mut monitors, "secondary", 11_920, -8_000);
        let canvas_size = monitor_canvas_size(&monitors);

        let origin = monitor_canvas_origin(&monitors, canvas_size);

        assert_eq!(origin, (9616.0, -8756.0));
    }

    #[test]
    fn normalize_monitors_keeps_layout_shape_centered_near_origin() {
        let mut monitors = Settings::default().monitors;
        offset_monitor(&mut monitors, "primary", 10_000, -8_000);
        offset_monitor(&mut monitors, "secondary", 11_920, -8_000);

        normalize_monitors_near_origin(&mut monitors);

        assert_eq!(
            monitor_arrangement_bounds(&monitors),
            Some((-1920, -540, 1920, 540))
        );
        assert!(monitor_borders_another(&monitors, "secondary", (0, -540)));
    }

    #[test]
    fn live_drag_moves_only_selected_monitor_until_release() {
        let widget = MonitorLayoutWidget::new(Settings::default().monitors, None);
        let drag = MonitorDrag {
            id: "secondary".to_string(),
            start_pointer: Point::from((0.0, 0.0)),
            start_x: 1920,
            start_y: 0,
            origin_x: 0.0,
            origin_y: 0.0,
            scale: 1.0,
            moved: true,
        };

        let live = widget.monitors_for_drag(&drag, Point::from((0.0, 1200.0)), false);
        let released = widget.monitors_for_drag(&drag, Point::from((0.0, 1200.0)), true);

        let live_primary = live.iter().find(|monitor| monitor.id == "primary").unwrap();
        let live_secondary = live
            .iter()
            .find(|monitor| monitor.id == "secondary")
            .unwrap();
        assert_eq!((live_primary.x, live_primary.y), (0, 0));
        assert_eq!((live_secondary.x, live_secondary.y), (1919, 1080));

        assert_eq!(
            monitor_arrangement_bounds(&released),
            Some((-1919, -1080, 1920, 1080))
        );
    }

    #[test]
    fn monitor_drag_snaps_gap_to_shared_border() {
        let monitors = Settings::default().monitors;

        let snapped = snap_monitor_to_neighbor(&monitors, "secondary", 2600, 120);

        assert_eq!(snapped, (1920, 120));
        assert!(monitor_borders_another(&monitors, "secondary", snapped));
    }

    #[test]
    fn monitor_drag_snaps_to_border_and_preserves_overlap() {
        let monitors = Settings::default().monitors;

        let snapped = snap_monitor_to_neighbor(&monitors, "secondary", 600, -1500);

        assert_eq!(snapped, (600, -1080));
        assert!(monitor_borders_another(&monitors, "secondary", snapped));
    }

    #[test]
    fn snap_position_rounds_to_nearest_grid_line() {
        assert_eq!(snap_position(14), 10);
        assert_eq!(snap_position(15), 20);
        assert_eq!(snap_position(-14), -10);
        assert_eq!(snap_position(-15), -20);
    }

    fn monitor_borders_another(monitors: &[MonitorConfig], id: &str, position: (i32, i32)) -> bool {
        let monitor = monitors.iter().find(|monitor| monitor.id == id).unwrap();
        monitors
            .iter()
            .any(|other| other.id != id && shares_border(position.0, position.1, monitor, other))
    }

    fn shares_border(x: i32, y: i32, monitor: &MonitorConfig, other: &MonitorConfig) -> bool {
        let width = monitor_logical_width(monitor);
        let height = monitor_logical_height(monitor);
        let other_width = monitor_logical_width(other);
        let other_height = monitor_logical_height(other);
        let horizontal_overlap = y < other.y + other_height && y + height > other.y;
        let vertical_overlap = x < other.x + other_width && x + width > other.x;
        let touches_left_or_right = x + width == other.x || x == other.x + other_width;
        let touches_top_or_bottom = y + height == other.y || y == other.y + other_height;

        (touches_left_or_right && horizontal_overlap) || (touches_top_or_bottom && vertical_overlap)
    }
}
