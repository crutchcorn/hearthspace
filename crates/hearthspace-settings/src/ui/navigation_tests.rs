use super::tests::{button_starting_with, state, view};
use crate::{
    state::{SettingsApp, SettingsCategory},
    test_support::{Ui, ids},
};
use masonry::{
    accesskit::{AriaCurrent, Role},
    core::{
        PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType, PointerUpdate,
        TextEvent, WidgetId, WindowEvent,
        keyboard::{Key, NamedKey},
        pointer::{PointerButton, PointerId},
    },
    kurbo::Rect,
};
use xilem::WidgetView;

fn bounds<V: WidgetView<SettingsApp>>(ui: &Ui<V>, id: WidgetId) -> Rect
where
    V::Widget: Sized,
{
    let widget = ui.harness.get_widget_with_id(id);
    widget
        .ctx()
        .window_transform()
        .transform_rect_bbox(widget.ctx().border_box())
}
fn role<V: WidgetView<SettingsApp>>(ui: &Ui<V>, role: Role) -> Option<WidgetId>
where
    V::Widget: Sized,
{
    ids(&ui.harness).into_iter().find(|id| {
        ui.harness
            .access_node(*id)
            .is_some_and(|node| node.role() == role)
    })
}
fn key<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>, key: NamedKey)
where
    V::Widget: Sized,
{
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(key)));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(key)));
}
fn resize<V: WidgetView<SettingsApp>>(
    ui: &mut Ui<V>,
    size: (u32, u32),
    build: &impl Fn(&SettingsApp) -> V,
) where
    V::Widget: Sized,
{
    ui.harness
        .process_window_event(WindowEvent::Resize(size.into()));
    ui.dispatch(build);
}
fn click<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>, x: f64, y: f64)
where
    V::Widget: Sized,
{
    ui.harness.mouse_move((x, y));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
}
// Native scroll portals/bars are also keyboard stops. The actionable sequence
// must remain identical to the reference, in both directions.
fn next_button<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>, reverse: bool) -> WidgetId
where
    V::Widget: Sized,
{
    for _ in 0..6 {
        ui.harness.press_tab_key(reverse);
        if let Some(id) = ui.harness.focused_widget_id()
            && ui
                .harness
                .access_node(id)
                .is_some_and(|node| node.role() == Role::Button)
        {
            return id;
        }
    }
    panic!("Tab did not reach a native button");
}

#[test]
fn exact_breakpoint_and_compact_overlay_geometry_keep_hidden_nodes_out_of_accessibility() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (560, 490));
    assert!(!ui.state.navigation.compact());
    let appearance = ui.button("Appearance");
    assert_eq!(bounds(&ui, appearance).x0, 12.0);
    assert_eq!(
        ui.harness.access_node(appearance).unwrap().aria_current(),
        Some(AriaCurrent::Page)
    );
    assert_eq!(
        ui.harness.access_node(appearance).unwrap().is_selected(),
        Some(true)
    );
    resize(&mut ui, (559, 490), &view);
    let toggle = ui.button("Settings categories");
    assert_eq!(bounds(&ui, toggle).y0, 4.0);
    assert_eq!(bounds(&ui, toggle).height(), 40.0);
    assert_eq!(
        ui.harness.access_node(toggle).unwrap().data().is_expanded(),
        Some(false)
    );
    assert_eq!(
        ui.harness.access_node(toggle).unwrap().controls().count(),
        0
    );
    assert!(ui.harness.access_node(appearance).is_none());
    assert!(role(&ui, Role::Navigation).is_none());
    ui.harness.focus_on(Some(toggle));
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    let navigation = role(&ui, Role::Navigation).unwrap();
    assert_eq!(bounds(&ui, navigation), Rect::new(0.0, 49.0, 559.0, 331.0));
    assert_eq!(bounds(&ui, appearance), Rect::new(8.0, 57.0, 551.0, 97.0));
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    let controlled: Vec<_> = ui
        .harness
        .access_node(toggle)
        .unwrap()
        .controls()
        .map(|node| node.id())
        .collect();
    assert_eq!(
        controlled,
        vec![ui.harness.access_node(navigation).unwrap().id()]
    );
    resize(&mut ui, (400, 420), &view);
    assert_eq!(bounds(&ui, navigation), Rect::new(0.0, 49.0, 400.0, 331.0));
    assert_eq!(bounds(&ui, appearance).x1, 392.0);
    key(&mut ui, NamedKey::Escape);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert!(role(&ui, Role::Navigation).is_none());
    assert_eq!(
        ui.harness.access_node(toggle).unwrap().controls().count(),
        0
    );
}

#[test]
fn keyboard_category_order_and_boundaries_remain_native_without_a_focus_trap() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (480, 490));
    let toggle = ui.button("Settings categories");
    ui.harness.focus_on(Some(toggle));
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    for name in [
        "Appearance",
        "Displays",
        "Background",
        "Gestures",
        "AI",
        "System",
    ] {
        assert_eq!(
            next_button(&mut ui, false),
            ui.button(name),
            "forward Tab: {name}"
        );
    }
    let light = button_starting_with(&ui, "Light");
    assert_eq!(
        next_button(&mut ui, false),
        light,
        "Tab exits navigation into the page"
    );
    assert_eq!(next_button(&mut ui, true), ui.button("System"));
    ui.harness.focus_on(Some(ui.button("Appearance")));
    assert_eq!(next_button(&mut ui, true), toggle);
    ui.harness
        .focus_on(Some(button_starting_with(&ui, "Olive")));
    assert_eq!(
        next_button(&mut ui, false),
        toggle,
        "last page control cycles to opener"
    );
    ui.harness.focus_on(Some(ui.button("Background")));
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    assert_eq!(ui.state.selected_category, SettingsCategory::Background);
    assert!(!ui.state.navigation.open);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    assert_eq!(next_button(&mut ui, false), ui.button("Upload photo"));
}

#[test]
fn compact_padding_and_row_gaps_shield_the_page_but_outside_clicks_do_not_dismiss() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (480, 490));
    let dark = bounds(&ui, button_starting_with(&ui, "Dark"));
    ui.harness
        .mouse_click_on(ui.button("Settings categories"), None);
    ui.dispatch(&view);
    let original = ui.state.settings.appearance;
    let status = ui.state.status.clone();
    let gap = (0..5)
        .map(|row| 99.0 + 45.0 * f64::from(row))
        .find(|y| *y > dark.y0 && *y < dark.y1)
        .expect("a navigation gap overlaps the dark mode card");
    click(&mut ui, dark.center().x, gap);
    ui.dispatch(&view);
    assert_eq!(ui.state.settings.appearance, original);
    assert_eq!(
        ui.state.status, status,
        "gap must not activate the underlying card"
    );
    assert!(ui.state.navigation.open);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Settings categories"))
    );
    key(&mut ui, NamedKey::Escape);
    ui.dispatch(&view);
    assert!(
        !ui.state.navigation.open,
        "Escape still closes after a blank gap click"
    );
    ui.harness
        .mouse_click_on(ui.button("Settings categories"), None);
    ui.dispatch(&view);
    click(&mut ui, 4.0, 70.0);
    ui.dispatch(&view);
    assert_eq!(ui.state.selected_category, SettingsCategory::Appearance);
    assert!(ui.state.navigation.open);
    click(&mut ui, 460.0, 470.0);
    ui.dispatch(&view);
    assert!(
        ui.state.navigation.open,
        "this navigation is not an outside-dismiss popup"
    );
}

#[test]
fn resizing_preserves_gesture_draft_widget_ids_and_focused_page_control() {
    let (_dir, mut state) = state();
    state.selected_category = SettingsCategory::Gestures;
    state.favorites.begin_add();
    let generation = state.favorites.recording.as_ref().unwrap().generation;
    state.favorites.stroke(
        "Alpha.desktop",
        generation,
        crate::favorites::tests::shape(),
    );
    let points = state.favorites.recording.as_ref().unwrap().points.clone();
    let mut ui = Ui::new(state, view, (680, 490));
    let editor = role(&ui, Role::EditableComboBox).unwrap();
    let pad = role(&ui, Role::Canvas).unwrap();
    ui.harness.focus_on(Some(editor));
    for size in [(400, 420), (680, 490), (559, 490), (560, 490)] {
        resize(&mut ui, size, &view);
        assert_eq!(role(&ui, Role::EditableComboBox), Some(editor));
        assert_eq!(role(&ui, Role::Canvas), Some(pad));
        assert_eq!(ui.harness.focused_widget_id(), Some(editor));
        let recording = ui.state.favorites.recording.as_ref().unwrap();
        assert_eq!(recording.generation, generation);
        assert_eq!(recording.points, points);
        assert!(recording.valid);
    }
}

#[test]
fn resizing_restores_hidden_navigation_focus_and_retains_open_state() {
    let (_dir, mut state) = state();
    state.selected_category = SettingsCategory::System;
    let mut ui = Ui::new(state, view, (680, 490));
    ui.harness.focus_on(Some(ui.button("System")));
    resize(&mut ui, (400, 420), &view);
    let toggle = ui.button("Settings categories");
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    assert!(ui.state.navigation.open);
    resize(&mut ui, (680, 490), &view);
    let focused = ui.harness.focused_widget_id().unwrap();
    assert_eq!(
        ui.harness.access_node(focused).unwrap().role(),
        Role::Button
    );
    assert_eq!(
        focused,
        ui.button("System"),
        "widening restores the current category"
    );
    assert!(ui.state.navigation.open);
    resize(&mut ui, (400, 420), &view);
    assert!(ui.state.navigation.open);
    assert!(role(&ui, Role::Navigation).is_some());
    key(&mut ui, NamedKey::Escape);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(toggle));
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
fn short_overlay_scrolls_without_selecting_a_category_during_touch_drag() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (400, 220));
    ui.activate(ui.button("Settings categories"));
    ui.dispatch(&view);
    let navigation = role(&ui, Role::Navigation).unwrap();
    assert_eq!(bounds(&ui, navigation), Rect::new(0.0, 49.0, 400.0, 220.0));
    let before = bounds(&ui, ui.button("System"));
    for (kind, y) in [
        ("down", 185.0),
        ("move", 150.0),
        ("move", 100.0),
        ("move", 60.0),
        ("up", 60.0),
    ] {
        ui.harness.process_pointer_event(touch(kind, 200.0, y));
    }
    ui.dispatch(&view);
    assert!(ui.state.navigation.open);
    assert_eq!(ui.state.selected_category, SettingsCategory::Appearance);
    let after = bounds(&ui, ui.button("System"));
    assert!(
        after.y0 < before.y0,
        "touch must scroll the retained navigation list"
    );
    assert!(after.y1 <= 220.0, "the last category must remain reachable");
    ui.harness.mouse_click_on(ui.button("System"), None);
    ui.dispatch(&view);
    assert_eq!(ui.state.selected_category, SettingsCategory::System);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Settings categories"))
    );
}
