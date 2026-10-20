use super::tests::{bounds as control_bounds, draw, named, state, view};
use crate::{state::SettingsApp, test_support::Ui};
use masonry::{
    accesskit::Role,
    core::{
        PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType, PointerUpdate,
        TextEvent, WindowEvent,
        keyboard::{Key, Modifiers, NamedKey},
        pointer::{PointerButton, PointerId},
    },
    kurbo::Rect,
};
use xilem::WidgetView;

fn nodes<V: WidgetView<SettingsApp>>(ui: &Ui<V>, role: Role) -> Vec<(String, Rect, bool)>
where
    V::Widget: Sized,
{
    let mut pending = vec![ui.harness.access_tree().state().root()];
    let mut result = vec![];
    while let Some(node) = pending.pop() {
        pending.extend(node.children());
        if node.role() == role {
            let bounds = node.bounding_box().expect("picker node bounds");
            result.push((
                node.label().or_else(|| node.value()).unwrap_or_default(),
                Rect::new(bounds.x0, bounds.y0, bounds.x1, bounds.y1),
                node.is_selected() == Some(true),
            ));
        }
    }
    result
}

fn key<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>, value: NamedKey)
where
    V::Widget: Sized,
{
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(value)));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(value)));
}

fn replace_text<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>, value: &str)
where
    V::Widget: Sized,
{
    let TextEvent::Keyboard(mut event) = TextEvent::key_down(Key::Character("a".into())) else {
        unreachable!()
    };
    event.modifiers = Modifiers::CONTROL;
    ui.harness.process_text_event(TextEvent::Keyboard(event));
    ui.harness.keyboard_type_chars(value);
}

#[test]
fn editable_picker_filters_and_selects_without_moving_the_pad_or_losing_the_draft() {
    let (_dir, mut state) = state();
    state.favorites.begin_add();
    let mut ui = Ui::new(state, view, (470, 490));
    let editor = named(&ui, "Open this app", Role::EditableComboBox);
    let bounds = ui
        .harness
        .access_node(editor)
        .unwrap()
        .bounding_box()
        .unwrap();
    let trigger = ui
        .harness
        .get_widget_with_id(ui.button("Choose app"))
        .ctx()
        .bounding_box();
    assert_eq!(
        bounds.height(),
        44.0,
        "the combobox exposes its full target, not only its text line"
    );
    assert_eq!((bounds.y0, bounds.y1), (trigger.y0, trigger.y1));
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
    assert_eq!(
        ui.harness.access_node(editor).unwrap().value().as_deref(),
        Some("Alpha")
    );
    draw(&mut ui, &[[30.0, 40.0], [90.0, 130.0], [150.0, 40.0]]);
    ui.dispatch(&view);
    let points = ui
        .state
        .favorites
        .recording
        .as_ref()
        .unwrap()
        .points
        .clone();
    let pad = named(&ui, "Gesture drawing area", Role::Canvas);
    let before = control_bounds(&ui, pad);
    let save = ui
        .harness
        .get_widget_with_id(ui.button("Save shortcut"))
        .ctx()
        .bounding_box();
    ui.activate(ui.button("Choose app"));
    ui.dispatch(&view);
    assert_eq!(nodes(&ui, Role::ListBox).len(), 1);
    assert_eq!(
        nodes(&ui, Role::ListBox)[0].1.y0,
        trigger.y1 + 7.0,
        "the popup anchors to the field border plus its six-pixel gap, excluding glow"
    );
    assert!(
        nodes(&ui, Role::ListBoxOption)
            .iter()
            .any(|(name, _, selected)| name == "Alpha" && *selected)
    );
    assert_eq!(control_bounds(&ui, pad), before);
    assert_eq!(
        ui.harness
            .get_widget_with_id(ui.button("Save shortcut"))
            .ctx()
            .bounding_box(),
        save
    );
    ui.harness
        .mouse_move(((bounds.x0 + bounds.x1) / 2.0, (bounds.y0 + bounds.y1) / 2.0));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert_eq!(
        nodes(&ui, Role::ListBox).len(),
        1,
        "the open popup must leave its text field editable"
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
    replace_text(&mut ui, "Beta");
    ui.dispatch(&view);
    let choices = nodes(&ui, Role::ListBoxOption);
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].0, "Beta");
    assert_eq!(ui.state.favorites.query, "Beta");
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    let recording = ui.state.favorites.recording.as_ref().unwrap();
    assert_eq!(recording.app_id, "Beta.desktop");
    assert_eq!(recording.points, points);
    assert!(recording.valid);
    assert!(nodes(&ui, Role::ListBox).is_empty());
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
    assert_eq!(
        ui.harness.access_node(editor).unwrap().value().as_deref(),
        Some("Beta")
    );
    assert_eq!(control_bounds(&ui, pad), before);
}

#[test]
fn arrow_navigation_escape_empty_query_and_outside_click_keep_committed_selection() {
    let (_dir, mut state) = state();
    state.favorites.begin_add();
    let mut ui = Ui::new(state, view, (470, 490));
    let editor = named(&ui, "Open this app", Role::EditableComboBox);
    key(&mut ui, NamedKey::ArrowDown);
    ui.dispatch(&view);
    key(&mut ui, NamedKey::ArrowDown);
    ui.dispatch(&view);
    assert_eq!(
        ui.harness
            .access_node(editor)
            .unwrap()
            .active_descendant()
            .unwrap()
            .label()
            .as_deref(),
        Some("Beta")
    );
    key(&mut ui, NamedKey::Escape);
    ui.dispatch(&view);
    assert_eq!(
        ui.state.favorites.recording.as_ref().unwrap().app_id,
        "Alpha.desktop"
    );
    assert!(nodes(&ui, Role::ListBox).is_empty());
    replace_text(&mut ui, "Nothing matches");
    ui.dispatch(&view);
    assert!(nodes(&ui, Role::ListBoxOption).is_empty());
    assert!(
        nodes(&ui, Role::Label)
            .iter()
            .any(|(name, _, _)| name == "No apps found")
    );
    let pad = named(&ui, "Gesture drawing area", Role::Canvas);
    let area = control_bounds(&ui, pad);
    ui.harness.mouse_move((area.x0 + 10.0, area.y1 - 10.0));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert!(nodes(&ui, Role::ListBox).is_empty());
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
    assert_eq!(
        ui.harness.access_node(editor).unwrap().value().as_deref(),
        Some("Alpha")
    );
    assert!(
        ui.state
            .favorites
            .recording
            .as_ref()
            .unwrap()
            .points
            .is_empty(),
        "the outside dismissal must not start a stroke"
    );
    ui.activate(ui.button("Choose app"));
    ui.dispatch(&view);
    key(&mut ui, NamedKey::Tab);
    ui.dispatch(&view);
    assert!(nodes(&ui, Role::ListBox).is_empty());
    assert_ne!(ui.harness.focused_widget_id(), Some(editor));
}

fn touch(kind: &str, x: f64, y: f64) -> PointerEvent {
    let pointer = PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Touch,
    };
    let state = PointerState {
        position: (x, y).into(),
        pressure: 0.5,
        ..Default::default()
    };
    match kind {
        "down" => PointerEvent::Down(PointerButtonEvent {
            pointer,
            state,
            button: None,
        }),
        "move" => PointerEvent::Move(PointerUpdate {
            pointer,
            current: state,
            coalesced: vec![],
            predicted: vec![],
        }),
        "up" => PointerEvent::Up(PointerButtonEvent {
            pointer,
            state,
            button: None,
        }),
        _ => unreachable!(),
    }
}

#[test]
fn popup_touch_scroll_does_not_select_and_tracks_resize_then_closes_with_editor() {
    let (dir, mut state) = state();
    for index in 0..20 {
        std::fs::write(
            dir.path()
                .join(format!("applications/Extra{index:02}.desktop")),
            format!("[Desktop Entry]\nType=Application\nName=Extra {index:02}\nExec=false\n"),
        )
        .unwrap();
    }
    state.favorites.catalog =
        hearthspace_shell::app_catalog::AppCatalog::load_from_data_dirs(vec![
            dir.path().to_owned(),
        ]);
    state.favorites.start("Extra19.desktop");
    let mut ui = Ui::new(state, view, (470, 490));
    ui.activate(ui.button("Choose app"));
    ui.dispatch(&view);
    let menu = nodes(&ui, Role::ListBox)[0].1;
    let selected = nodes(&ui, Role::ListBoxOption)
        .into_iter()
        .find(|(_, _, selected)| *selected)
        .unwrap()
        .1;
    assert!(
        selected.y0 >= menu.y0 && selected.y1 <= menu.y1,
        "opening reveals the selected item"
    );
    let x = menu.x0 + 35.0;
    let y = menu.y0 + 80.0;
    let before = nodes(&ui, Role::ListBoxOption)
        .into_iter()
        .find(|(name, _, _)| name == "Extra 19")
        .unwrap()
        .1;
    for (kind, offset) in [("down", 0.0), ("move", 70.0), ("up", 80.0)] {
        ui.harness.process_pointer_event(touch(kind, x, y + offset));
    }
    ui.dispatch(&view);
    assert!(ui.state.favorites.picker_open);
    assert_eq!(
        ui.state.favorites.recording.as_ref().unwrap().app_id,
        "Extra19.desktop"
    );
    let after = nodes(&ui, Role::ListBoxOption)
        .into_iter()
        .find(|(name, _, _)| name == "Extra 19")
        .unwrap()
        .1;
    assert!(after.y0 > before.y0, "touch dragging moves list content");
    ui.harness
        .process_window_event(WindowEvent::Resize((300, 420).into()));
    ui.dispatch(&view);
    let menu = nodes(&ui, Role::ListBox)[0].1;
    assert!(menu.x0 >= 0.0 && menu.x1 <= 300.0 && menu.y0 >= 0.0 && menu.y1 <= 420.0);
    ui.state.favorites.cancel();
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(
        nodes(&ui, Role::ListBox).is_empty(),
        "unmounting the editor removes its attached popup"
    );
}
