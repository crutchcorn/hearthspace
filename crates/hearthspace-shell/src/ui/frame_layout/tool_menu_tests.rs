use super::*;
use hearthspace_ipc::command::CanvasTool;
use masonry::{
    accesskit::Toggled,
    core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    },
    properties::BorderColor,
};

#[path = "tool_output_tests.rs"]
mod outputs;

const SELECT_TRIGGER: &str = "Select desktop items, choose canvas tool";
const PAN_TRIGGER: &str = "Pan canvas, choose canvas tool";

fn fixture(tool: CanvasTool) -> (ShellState, mpsc::Receiver<task::Request>) {
    let mut state = state();
    let mut snapshot = state.frame_layout.snapshot.clone();
    snapshot.editing = false;
    snapshot.canvas_tool = Some(tool);
    snapshot.outputs[0].rect = Rect {
        width: 1280.0,
        height: 800.0,
        ..Default::default()
    };
    apply(&mut state, snapshot);
    let (sender, requests) = mpsc::channel(32);
    state.frame_layout.requests = Some(sender);
    (state, requests)
}

fn menu(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(tool_menu::panel(state, &state.outputs[0]))
}

fn key<V: WidgetView<ShellState, Widget: Sized>>(
    ui: &mut Ui<V>,
    key: Key,
    build: &impl Fn(&ShellState) -> V,
) {
    ui.harness
        .process_text_event(TextEvent::key_down(key.clone()));
    ui.dispatch(build);
    ui.harness.process_text_event(TextEvent::key_up(key));
    ui.dispatch(build);
}

fn toggled<V: WidgetView<ShellState, Widget: Sized>>(ui: &Ui<V>, name: &str) -> Option<Toggled> {
    ui.harness.access_node(ui.button(name)).unwrap().toggled()
}

fn observe(state: &mut ShellState, tool: Option<CanvasTool>) {
    let mut snapshot = state.frame_layout.snapshot.clone();
    snapshot.canvas_tool = tool;
    apply(state, snapshot);
}

#[test]
fn primary_choices_have_names_hints_native_keyboard_and_confirmed_pressed_state() {
    let (mut state, mut requests) = fixture(CanvasTool::Select);
    state.outputs[0].show_controls = true;
    state.outputs[0].quick_panel = crate::ui::quick_panel::Kind::Tools;
    assert_eq!(
        tool_menu::size(&state, &state.outputs[0]),
        xilem::dpi::LogicalSize::new(260.0, 144.0)
    );
    let mut ui = Ui::new(state, menu, (260, 144));
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Select desktop items"))
    );
    assert_eq!(toggled(&ui, "Select desktop items"), Some(Toggled::True));
    assert_eq!(toggled(&ui, "Pan canvas"), Some(Toggled::False));
    let names: Vec<_> = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| ui.harness.access_node(id))
        .filter_map(|node| node.label().or_else(|| node.value()))
        .collect();
    for hint in ["V", "H", "/"] {
        assert!(names.iter().any(|name| name == hint), "missing {hint}");
    }
    let buttons = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| ui.harness.access_node(id))
        .filter(|node| node.role() == Role::Button)
        .count();
    assert_eq!(buttons, 3);
    key(&mut ui, Key::Named(NamedKey::ArrowDown), &menu);
    let pan = ui.button("Pan canvas");
    assert_eq!(ui.harness.focused_widget_id(), Some(pan));
    let space = Key::Character(" ".into());
    ui.harness
        .process_text_event(TextEvent::key_down(space.clone()));
    ui.dispatch(&menu);
    assert!(
        requests.try_recv().is_err(),
        "a held activation must not select yet"
    );
    ui.harness.process_text_event(TextEvent::key_up(space));
    ui.dispatch(&menu);
    let request = requests.try_recv().unwrap();
    assert_eq!(request.command, "canvas-tool pan");
    assert!(
        ui.state.outputs[0].show_controls,
        "keep the chooser available until confirmation"
    );
    assert_eq!(
        toggled(&ui, "Pan canvas"),
        Some(Toggled::False),
        "queued work must not pretend to be selected"
    );
    observe(&mut ui.state, Some(CanvasTool::Pan));
    request.selection.unwrap().complete(&mut ui.state, Ok(()));
    assert!(!ui.state.outputs[0].show_controls);
    ui.rebuild(&menu);
    ui.dispatch(&menu);
    assert_eq!(toggled(&ui, "Pan canvas"), Some(Toggled::True));
    assert_eq!(toggled(&ui, "Select desktop items"), Some(Toggled::False));
    assert_eq!(
        ui.button("Pan canvas"),
        pan,
        "confirmation preserves native control identity"
    );
}

#[test]
fn remembered_primary_restores_before_the_next_click_opens_its_menu() {
    let (mut state, mut requests) = fixture(CanvasTool::Pan);
    observe(&mut state, Some(CanvasTool::Group));
    let mut ui = Ui::new(state, tools, (188, 44));
    let trigger = ui.button(PAN_TRIGGER);
    assert_eq!(toggled(&ui, PAN_TRIGGER), Some(Toggled::False));
    ui.activate(trigger);
    ui.dispatch(&tools);
    assert_eq!(requests.try_recv().unwrap().command, "canvas-tool pan");
    assert!(!ui.state.outputs[0].show_controls);
    assert_eq!(
        ui.state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Group)
    );
    observe(&mut ui.state, Some(CanvasTool::Pan));
    ui.rebuild(&tools);
    ui.dispatch(&tools);
    assert_eq!(ui.button(PAN_TRIGGER), trigger);
    assert_eq!(toggled(&ui, PAN_TRIGGER), Some(Toggled::True));
    ui.activate(trigger);
    ui.dispatch(&tools);
    let request = requests.try_recv().unwrap();
    assert!(request.command.starts_with("frame-popup-anchor "));
    assert!(request.command.contains("quick-panel"));
    assert!(request.open.is_some());
    assert!(
        !ui.state.outputs[0].show_controls,
        "native popup waits for its anchor acknowledgement"
    );
}

#[test]
fn trigger_enter_opens_without_selecting_and_has_visible_keyboard_focus() {
    let (state, mut requests) = fixture(CanvasTool::Select);
    let mut ui = Ui::new(state, tools, (188, 44));
    let trigger = ui.button(SELECT_TRIGGER);
    ui.harness.focus_on(Some(trigger));
    ui.dispatch(&tools);
    while requests.try_recv().is_ok() {}
    assert_eq!(
        ui.harness
            .get_widget_with_id(trigger)
            .get_prop::<BorderColor>()
            .color,
        hearthspace_ui_components::FOCUS_RING_ON_GLASS
    );
    let before = ui.harness.access_node(trigger).unwrap().bounding_box();
    key(&mut ui, Key::Named(NamedKey::Enter), &tools);
    assert!(
        requests
            .try_recv()
            .unwrap()
            .command
            .starts_with("frame-popup-anchor ")
    );
    assert!(requests.try_recv().is_err());
    assert_eq!(
        before,
        ui.harness.access_node(trigger).unwrap().bounding_box()
    );
}

#[test]
fn selection_button_sends_command_and_dismisses_other_popups() {
    let (mut state, mut requests) = fixture(CanvasTool::Select);
    state.outputs[0].show_map = true;
    let mut ui = Ui::new(state, tools, (188, 44));
    ui.activate(ui.button("Select window group"));
    ui.dispatch(&tools);
    assert!(!ui.state.outputs[0].show_map);
    assert_eq!(requests.try_recv().unwrap().command, "canvas-tool group");
    assert!(ui.state.frame_layout.error.is_none());
    assert_eq!(toggled(&ui, "Select window group"), Some(Toggled::False));
    observe(&mut ui.state, Some(CanvasTool::Group));
    ui.rebuild(&tools);
    ui.dispatch(&tools);
    assert_eq!(toggled(&ui, "Select window group"), Some(Toggled::True));
}

#[test]
fn compact_menu_keeps_all_six_rows_reachable_when_height_is_constrained() {
    let (mut state, mut requests) = fixture(CanvasTool::Pan);
    state.frame_layout.snapshot.outputs[0].compact = true;
    assert_eq!(
        tool_menu::size(&state, &state.outputs[0]),
        xilem::dpi::LogicalSize::new(260.0, 276.0)
    );
    state.frame_layout.snapshot.outputs[0].rect = Rect {
        width: 240.0,
        height: 160.0,
        ..Default::default()
    };
    state.outputs[0].show_controls = true;
    state.outputs[0].quick_panel = crate::ui::quick_panel::Kind::Tools;
    assert_eq!(
        tool_menu::size(&state, &state.outputs[0]),
        xilem::dpi::LogicalSize::new(216.0, 136.0)
    );
    let mut ui = Ui::new(state, menu, (216, 136));
    for name in [
        "Select desktop items",
        "Pan canvas",
        "Search canvas",
        "Select window group",
        "Change layout",
        "Canvas map",
    ] {
        ui.button(name);
    }
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Pan canvas"))
    );
    key(&mut ui, Key::Named(NamedKey::End), &menu);
    let map = ui.button("Canvas map");
    assert_eq!(ui.harness.focused_widget_id(), Some(map));
    let bounds = ui.harness.access_node(map).unwrap().bounding_box().unwrap();
    assert!(
        bounds.y0 >= 6.0 && bounds.y1 <= 130.0 && bounds.x1 <= 216.0,
        "focused offscreen row must scroll into the viewport: {bounds:?}"
    );
    assert_eq!(
        toggled(&ui, "Canvas map"),
        None,
        "the map is an action, not a selected pointer tool"
    );
    key(&mut ui, Key::Named(NamedKey::ArrowUp), &menu);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Change layout"))
    );
    key(&mut ui, Key::Named(NamedKey::Escape), &menu);
    assert!(!ui.state.outputs[0].show_controls);
    assert!(
        requests.try_recv().is_err(),
        "navigation and dismissal must not select a tool"
    );
}

fn vertical(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    let chain = &state.frame_layout.chains[&widget_key("test", "tools")];
    sized_box(focus::boundary(
        widgets::content(
            state,
            &state.outputs[0],
            "tools",
            true,
            false,
            Default::default(),
            chain,
        ),
        chain.clone(),
        "test".into(),
        LayoutTarget::widget("tools"),
        state.frame_layout.snapshot.focus_request.clone(),
    ))
}

#[test]
fn side_toolbar_keeps_upright_controls_and_reveals_each_keyboard_focus() {
    let (state, mut requests) = fixture(CanvasTool::Select);
    assert_eq!(
        widgets::size(
            &state,
            "tools",
            true,
            false,
            true,
            Default::default(),
            false
        ),
        (44.0, 188.0)
    );
    let mut ui = Ui::new(state, vertical, (44, 188));
    ui.state.frame_layout.snapshot.focus_request = Some(FocusRequest {
        serial: 1,
        output: "test".into(),
        target: LayoutTarget::widget("tools"),
        reverse: false,
    });
    ui.rebuild(&vertical);
    ui.dispatch(&vertical);
    for name in [
        SELECT_TRIGGER,
        "Select window group",
        "Change layout",
        "Canvas map",
    ] {
        let button = ui.button(name);
        assert_eq!(ui.harness.focused_widget_id(), Some(button));
        let bounds = ui
            .harness
            .access_node(button)
            .unwrap()
            .bounding_box()
            .unwrap();
        assert_eq!((bounds.width(), bounds.height()), (44.0, 44.0));
        assert!(bounds.x0 >= 0.0 && bounds.x1 <= 44.0 && bounds.y1 <= 188.0);
        ui.harness.press_tab_key(false);
        ui.dispatch(&vertical);
    }
    let mut regions = Vec::new();
    while let Ok(request) = requests.try_recv() {
        if let Some(json) = request.command.strip_prefix("frame-focus-region ") {
            let value: serde_json::Value = serde_json::from_str(json).unwrap();
            regions.push(value["rect"]["y"].as_f64().unwrap());
        } else {
            assert!(request.command.starts_with("frame-focus-next "));
        }
    }
    assert_eq!(regions.len(), 4);
    assert!(regions.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn explicit_pan_selection_uses_the_asynchronous_queue_without_optimistic_selection() {
    let (mut state, mut requests) = fixture(CanvasTool::Search);
    select_tool(&mut state, CanvasTool::Pan);
    assert_eq!(requests.try_recv().unwrap().command, "canvas-tool pan");
    assert_eq!(
        state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Search)
    );
}
