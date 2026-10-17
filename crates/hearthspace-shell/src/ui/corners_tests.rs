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
