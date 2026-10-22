use super::*;
use crate::ui::test_support::{Ui, ids};
use hearthspace_common::frame_layout::{FrameEdge, ObjectPlacement, TargetKind};
use xilem::{WidgetView, masonry::accesskit::Role, view::sized_box};

#[path = "compact_tests.rs"]
mod compact;
#[path = "resize_tests.rs"]
mod resize;
#[path = "startup_tests.rs"]
mod startup;
#[path = "tray_tests.rs"]
mod tray;

fn state() -> ShellState {
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    apply(
        &mut state,
        Snapshot {
            version: 1,
            revision: 1,
            editing: true,
            session: 9,
            output: Some("test".into()),
            outputs: vec![Output {
                output: "test".into(),
                ..Default::default()
            }],
            editor_max_width: 260.0,
            editor_max_height: 600.0,
            confirmation_max_width: 420.0,
            confirmation_max_height: 600.0,
            ..Default::default()
        },
    );
    state
}
fn tools(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    let chain = &state.frame_layout.chains[&widget_key("test", "tools")];
    let inner = widgets::content(
        state,
        &state.outputs[0],
        "tools",
        false,
        false,
        Default::default(),
        chain,
    );
    sized_box(focus::boundary(
        inner,
        chain.clone(),
        "test".into(),
        LayoutTarget::widget("tools"),
        state.frame_layout.snapshot.focus_request.clone(),
    ))
}
fn inspector(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(editor::view(state))
}
fn dialog(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(confirm::view(state, 420.0, 300.0))
}

#[test]
fn native_tab_traverses_toolbar_controls_then_requests_the_next_surface() {
    let mut state = state();
    let (sender, mut requests) = mpsc::channel(8);
    state.frame_layout.requests = Some(sender);
    let mut ui = Ui::new(state, tools, (236, 44));
    ui.state.frame_layout.snapshot.focus_request = Some(FocusRequest {
        serial: 1,
        output: "test".into(),
        target: LayoutTarget::widget("tools"),
        reverse: false,
    });
    ui.rebuild(&tools);
    ui.dispatch(&tools);
    for name in [
        "Select desktop items",
        "Search canvas",
        "Select window group",
        "Change layout",
        "Canvas map",
    ] {
        assert_eq!(
            ui.harness.focused_widget_id(),
            Some(ui.button(name)),
            "{name}"
        );
        ui.harness.press_tab_key(false);
        ui.dispatch(&tools);
    }
    let mut revealed = 0;
    let request = loop {
        let request = requests.try_recv().unwrap();
        if let Some(value) = request.command.strip_prefix("frame-focus-region ") {
            let value: serde_json::Value = serde_json::from_str(value).unwrap();
            assert_eq!(value["target"]["id"], "tools");
            assert!(value["rect"]["width"].as_f64().unwrap() > 0.0);
            revealed += 1;
        } else {
            break request;
        }
    };
    assert_eq!(
        revealed, 5,
        "every focused toolbar control must be revealed"
    );
    assert!(request.command.starts_with("frame-focus-next "));
    let value: serde_json::Value =
        serde_json::from_str(request.command.split_once(' ').unwrap().1).unwrap();
    assert_eq!(value["target"]["id"], "tools");
    assert_eq!(value["reverse"], false);
    ui.state.frame_layout.snapshot.focus_request = Some(FocusRequest {
        serial: 2,
        output: "test".into(),
        target: LayoutTarget::widget("tools"),
        reverse: true,
    });
    ui.rebuild(&tools);
    ui.dispatch(&tools);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Canvas map"))
    );
}

#[test]
fn placement_inspector_sends_session_guarded_actions_and_real_date_choices() {
    let mut state = state();
    state.frame_layout.snapshot.selected = Some(Selection {
        output: "test".into(),
        target: LayoutTarget::widget("clock"),
    });
    state.frame_layout.snapshot.selected_title = "Clock".into();
    let (sender, mut requests) = mpsc::channel(8);
    state.frame_layout.requests = Some(sender);
    let mut ui = Ui::new(state, inspector, (260, 452));
    ui.activate(ui.button("Canvas"));
    ui.dispatch(&inspector);
    let command = requests.try_recv().unwrap().command;
    let payload: serde_json::Value =
        serde_json::from_str(command.strip_prefix("layout-command ").unwrap()).unwrap();
    assert_eq!(payload["session"], 9);
    assert_eq!(payload["action"], "place");
    assert_eq!(payload["placement"]["surface"], "canvas");
    assert_eq!(
        ui.harness
            .access_node(ui.button("Frame"))
            .unwrap()
            .toggled(),
        Some(masonry::accesskit::Toggled::True)
    );
    assert_eq!(
        ui.harness
            .access_node(ui.button("Canvas"))
            .unwrap()
            .toggled(),
        Some(masonry::accesskit::Toggled::False)
    );
    let choices = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| ui.harness.access_node(id))
        .filter(|node| node.role() == Role::RadioButton)
        .count();
    assert_eq!(choices, 5);
    let indicators: Vec<_> = ids(&ui.harness)
        .into_iter()
        .filter_map(|id| {
            let widget = ui.harness.get_widget_with_id(id);
            widget
                .downcast::<masonry::widgets::Canvas>()
                .map(|canvas| canvas.ctx().border_box())
        })
        .collect();
    assert_eq!(indicators.len(), 5);
    assert!(
        indicators
            .iter()
            .all(|rect| rect.width() == 14.0 && rect.height() == 14.0)
    );
    for name in ["Remove", "Done changing layout"] {
        let bounds = ui
            .harness
            .access_node(ui.button(name))
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(
            bounds.x0 >= 0.0 && bounds.x1 <= 260.0 && bounds.y0 >= 0.0 && bounds.y1 <= 54.0,
            "{name}: {bounds:?}"
        );
        assert_eq!(bounds.width(), 44.0, "{name} keeps its reference hit width");
    }
    for name in ["Shrink to icon", "Remove from group"] {
        assert_eq!(
            ui.harness
                .access_node(ui.button(name))
                .unwrap()
                .bounding_box()
                .unwrap()
                .width(),
            123.0,
            "options divide the available row equally"
        );
    }
}

#[test]
fn confirmation_focuses_cancel_and_does_not_enable_hiding_tools_implicitly() {
    let mut state = state();
    state.frame_layout.snapshot.confirmation = Some(Confirmation {
        kind: "hide-tools".into(),
        window_id: None,
    });
    let (sender, mut requests) = mpsc::channel(8);
    state.frame_layout.requests = Some(sender);
    let mut ui = Ui::new(state, dialog, (420, 300));
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Cancel")));
    assert!(
        ids(&ui.harness)
            .into_iter()
            .filter_map(|id| ui.harness.access_node(id))
            .any(|node| node.role() == Role::Dialog && node.is_modal())
    );
    ui.activate(ui.button("Open settings"));
    ui.dispatch(&dialog);
    let command = requests.try_recv().unwrap().command;
    assert!(command.contains("\"action\":\"confirm\""));
    assert!(
        !ui.state.frame_layout.snapshot.outputs[0]
            .layout
            .allow_hide_tools
    );
    ui.harness
        .process_text_event(masonry::core::TextEvent::key_down(
            masonry::core::keyboard::Key::Named(masonry::core::keyboard::NamedKey::Escape),
        ));
    ui.dispatch(&dialog);
    assert!(
        requests
            .try_recv()
            .unwrap()
            .command
            .contains("\"action\":\"cancel\"")
    );
}

#[test]
fn placing_or_locking_keeps_control_identities_but_removes_editor_and_private_selection() {
    let mut state = state();
    let id = state.frame_layout.windows[&widget_key("test", "clock")];
    let mut snapshot = state.frame_layout.snapshot.clone();
    snapshot.outputs[0].layout.set_placement(
        &LayoutTarget::widget("clock"),
        ObjectPlacement::frame(FrameEdge::Left, 0.5),
    );
    snapshot.revision += 1;
    apply(&mut state, snapshot);
    assert_eq!(state.frame_layout.windows[&widget_key("test", "clock")], id);
    apply(
        &mut state,
        Snapshot {
            version: 1,
            ..Default::default()
        },
    );
    assert_eq!(state.frame_layout.windows[&widget_key("test", "clock")], id);
    assert!(!state.frame_layout.visible);
    assert!(state.frame_layout.editor.is_none());
    assert!(state.frame_layout.snapshot.selected.is_none());
    assert!(!state.frame_layout.snapshot.outputs.is_empty());
}

#[test]
fn compact_frame_items_are_named_single_actions_at_reference_dimensions() {
    let mut state = state();
    let target = LayoutTarget::new(TargetKind::Item, "app:settings.desktop");
    state.frame_layout.snapshot.outputs[0]
        .layout
        .set_placement(&target, ObjectPlacement::frame(FrameEdge::Top, 0.5));
    let build = |state: &ShellState| {
        sized_box(items::view(
            state,
            crate::ui::desktop_item::TileContent {
                name: "Settings".into(),
                artwork: crate::ui::desktop_item::Artwork::Settings,
            },
            target.id.clone(),
            "test".into(),
            false,
        ))
    };
    let ui = Ui::new(state, build, (54, 54));
    let bounds = ui
        .harness
        .access_node(ui.button("Settings"))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!((bounds.width(), bounds.height()), (54.0, 54.0));
}

#[test]
fn editor_hints_follow_native_hover_focus_and_activation_without_stealing_focus() {
    let build = |_: &ShellState| {
        sized_box(tooltip::hint(
            xilem::view::button(xilem::view::label("Native action"), |_: &mut ShellState| {}),
            "Place on canvas",
        ))
    };
    let mut ui = Ui::new(state(), build, (180, 44));
    let button = ui.button("Native action");
    ui.harness.mouse_move_to(button);
    ui.dispatch(&build);
    assert_eq!(ui.state.frame_layout.tooltip.label, Some("Place on canvas"));
    assert!(ui.harness.focused_widget_id().is_none());
    ui.harness.mouse_move((-20.0, -20.0));
    ui.dispatch(&build);
    assert!(ui.state.frame_layout.tooltip.label.is_none());
    ui.harness.focus_on(Some(button));
    ui.dispatch(&build);
    assert_eq!(ui.state.frame_layout.tooltip.label, Some("Place on canvas"));
    assert_eq!(ui.harness.focused_widget_id(), Some(button));
    ui.activate(button);
    ui.dispatch(&build);
    assert!(ui.state.frame_layout.tooltip.label.is_none());
    assert_eq!(ui.harness.focused_widget_id(), Some(button));
    ui.harness.focus_on(None);
    ui.dispatch(&build);
    ui.harness.focus_on(Some(button));
    ui.dispatch(&build);
    assert!(ui.state.frame_layout.tooltip.label.is_some());
    apply(
        &mut ui.state,
        Snapshot {
            version: 1,
            ..Default::default()
        },
    );
    assert!(ui.state.frame_layout.tooltip.label.is_none());
}

#[test]
fn compact_tools_open_same_native_actions_and_use_current_tool_focus() {
    let mut state = state();
    state.frame_layout.snapshot.editing = false;
    state.frame_layout.snapshot.outputs[0].compact = true;
    let mut ui = Ui::new(state, tools, (44, 44));
    assert_eq!(
        widgets::size(
            &ui.state,
            "tools",
            false,
            false,
            true,
            Default::default(),
            true
        ),
        (44.0, 44.0)
    );
    ui.activate(ui.button("Canvas tools"));
    ui.dispatch(&tools);
    assert!(ui.state.outputs[0].show_controls);
    assert_eq!(
        ui.state.outputs[0].quick_panel,
        crate::ui::quick_panel::Kind::Tools
    );
    let menu = |state: &ShellState| sized_box(tool_menu::panel(state, &state.outputs[0]));
    let (sender, mut requests) = mpsc::channel(8);
    ui.state.frame_layout.requests = Some(sender);
    let mut menu_ui = Ui::new(ui.state, menu, (260, 232));
    assert_eq!(
        menu_ui.harness.focused_widget_id(),
        Some(menu_ui.button("Select desktop items"))
    );
    menu_ui.activate(menu_ui.button("Change layout"));
    menu_ui.dispatch(&menu);
    assert!(!menu_ui.state.outputs[0].show_controls);
    assert_eq!(requests.try_recv().unwrap().command, "layout-tool");
}
