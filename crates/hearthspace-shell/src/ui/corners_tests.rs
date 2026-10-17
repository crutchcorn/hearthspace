use super::*;
use crate::ui::test_support::Ui;
use masonry::{
    core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    },
    properties::BorderColor,
};
use xilem::view::sized_box;

fn notifications(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(notifications_view(state, &state.outputs[0]))
}
fn canvas(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(canvas_view(state, &state.outputs[0]))
}
fn system(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(system_view(state, &state.outputs[0]))
}
fn apps(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(apps_view(state, &state.outputs[0]))
}
fn state() -> ShellState {
    ShellState::new("/unused".into(), vec!["eDP-1".into()])
}

#[test]
fn notification_and_canvas_controls_open_their_own_panels() {
    let mut ui = Ui::new(state(), notifications, (48, 48));
    ui.harness.mouse_click_on(ui.button("Notifications"), None);
    ui.dispatch(&notifications);
    assert!(ui.state.outputs[0].show_notifications);
    ui.harness.mouse_click_on(ui.button("Notifications"), None);
    ui.dispatch(&notifications);
    assert!(!ui.state.outputs[0].show_notifications);

    let mut ui = Ui::new(state(), canvas, (88, 48));
    for name in ["Canvas map", "Select window group"] {
        let bounds = ui
            .harness
            .access_node(ui.button(name))
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(
            bounds.x0 >= 0.0 && bounds.x1 <= 88.0 && bounds.y1 <= 48.0,
            "{name}: {bounds:?}"
        );
    }
    ui.harness.mouse_click_on(ui.button("Canvas map"), None);
    ui.dispatch(&canvas);
    assert!(ui.state.outputs[0].show_map);
}

#[test]
fn apps_list_has_visible_keyboard_focus_and_opens_with_enter() {
    let mut ui = Ui::new(state(), apps, (52, 48));
    let button = ui.button("Apps List");
    let unfocused = ui.harness.render();
    let before = ui.harness.access_node(button).unwrap().bounding_box();
    ui.harness.focus_on(Some(button));
    assert_ne!(
        unfocused,
        ui.harness.render(),
        "keyboard focus must be visible"
    );
    assert_eq!(
        ui.harness
            .get_widget_with_id(button)
            .get_prop::<BorderColor>()
            .color,
        components::ACCENT
    );
    assert_eq!(
        before,
        ui.harness.access_node(button).unwrap().bounding_box(),
        "focus must not move controls"
    );
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&apps);
    assert!(ui.state.outputs[0].show_search);
}

#[test]
fn narrow_system_corner_keeps_tray_battery_and_controls_reachable() {
    use crate::battery::{BatteryKind, BatteryState, BatteryStatus};
    let mut state = state();
    state.battery = Some(BatteryStatus {
        percentage: 57,
        state: BatteryState::Discharging,
        kind: BatteryKind::Battery,
        on_battery: true,
        warning: None,
    });
    state.tray.snapshot.items = (1..=20).map(crate::ui::tray_tests::item).collect();
    let mut ui = Ui::new(state, system, (236, 48));
    for name in ["Application tray", "Control center", "Power"] {
        let button = ui.button(name);
        let bounds = ui
            .harness
            .access_node(button)
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(
            bounds.x0 >= 0.0 && bounds.x1 <= 236.0 && bounds.y1 <= 48.0,
            "{name}: {bounds:?}"
        );
        ui.harness.mouse_click_on(button, None);
        ui.dispatch(&system);
        let output = &ui.state.outputs[0];
        assert_eq!(
            [output.tray.open, output.show_controls, output.show_power]
                .into_iter()
                .filter(|open| *open)
                .count(),
            1
        );
    }
}

#[test]
fn corner_controls_remain_centered_when_the_surface_has_extra_width() {
    let ui = Ui::new(state(), notifications, (64, 48));
    let bounds = ui
        .harness
        .access_node(ui.button("Notifications"))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!((bounds.x0 + bounds.x1) / 2.0, 32.0);

    let ui = Ui::new(state(), canvas, (108, 48));
    let left = ui
        .harness
        .access_node(ui.button("Canvas map"))
        .unwrap()
        .bounding_box()
        .unwrap();
    let right = ui
        .harness
        .access_node(ui.button("Select window group"))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!((left.x0 + right.x1) / 2.0, 54.0);

    let ui = Ui::new(state(), apps, (64, 48));
    let bounds = ui
        .harness
        .access_node(ui.button("Apps List"))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!((bounds.x0 + bounds.x1) / 2.0, 32.0);
    assert_eq!(
        apps_width(&ui.state),
        48.0,
        "an empty dock must not reserve a gap"
    );
}

#[test]
fn system_corner_requests_only_its_content_width_and_updates_for_tray_items() {
    let mut state = state();
    state.clock = "Sat Oct 17 02:38".into();
    let mut ui = Ui::new(state, system, (250, 48));
    let empty_width = system_width(&ui.state, &ui.state.outputs[0]);
    assert!(empty_width < 220.0, "spare clock width: {empty_width}");
    ui.state.tray.snapshot.items = vec![crate::ui::tray_tests::item(1)];
    ui.rebuild(&system);
    ui.dispatch(&system);
    let populated_width = system_width(&ui.state, &ui.state.outputs[0]);
    assert!(
        (populated_width - empty_width - 78.0).abs() <= 1.0,
        "one icon + drawer + intervening gaps: {empty_width} -> {populated_width}"
    );
    ui.harness
        .process_window_event(masonry::core::WindowEvent::Resize(
            (populated_width as u32, 48).into(),
        ));
    ui.dispatch(&system);
    ui.button("App 1");
    ui.state.tray.snapshot.items.clear();
    ui.rebuild(&system);
    ui.dispatch(&system);
    assert_eq!(system_width(&ui.state, &ui.state.outputs[0]), empty_width);
}
