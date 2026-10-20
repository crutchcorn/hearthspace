use super::*;
use crate::ui::test_support::{Ui, ids};
use masonry::{
    core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    },
    properties::{Background, BorderColor},
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
    let mut ui = Ui::new(state(), notifications, (108, 54));
    ui.harness.mouse_click_on(ui.button("Notifications"), None);
    ui.dispatch(&notifications);
    assert!(ui.state.outputs[0].show_notifications);
    ui.harness.mouse_click_on(ui.button("Notifications"), None);
    ui.dispatch(&notifications);
    assert!(!ui.state.outputs[0].show_notifications);

    let mut ui = Ui::new(state(), canvas, (102, 54));
    for name in ["Canvas map", "Select window group"] {
        let bounds = ui
            .harness
            .access_node(ui.button(name))
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(
            bounds.x0 >= 0.0 && bounds.x1 <= 102.0 && bounds.y1 <= 54.0,
            "{name}: {bounds:?}"
        );
    }
    ui.harness.mouse_click_on(ui.button("Canvas map"), None);
    ui.dispatch(&canvas);
    assert!(ui.state.outputs[0].show_map);
}

#[test]
fn apps_list_has_visible_keyboard_focus_and_opens_with_enter() {
    let mut ui = Ui::new(state(), apps, (54, 54));
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
        components::TEXT
    );
    assert_eq!(
        before,
        ui.harness.access_node(button).unwrap().bounding_box(),
        "focus must not move controls"
    );
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(false));
    assert_eq!(ui.harness.focused_widget_id(), None);
    assert_eq!(
        ui.harness
            .get_widget_with_id(button)
            .get_prop::<BorderColor>()
            .color,
        xilem::Color::TRANSPARENT,
        "an inactive corner must not retain a keyboard ring"
    );
    ui.harness
        .process_text_event(TextEvent::WindowFocusChange(true));
    ui.harness.press_tab_key(false);
    assert_eq!(ui.harness.focused_widget_id(), Some(button));
    assert_eq!(
        ui.harness
            .get_widget_with_id(button)
            .get_prop::<BorderColor>()
            .color,
        components::TEXT
    );
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&apps);
    assert!(ui.state.outputs[0].show_search);
}

#[test]
fn translucent_corners_keep_idle_buttons_clear_and_focus_contrasting() {
    fn check<V: WidgetView<ShellState>>(
        state: ShellState,
        build: impl Fn(&ShellState) -> V,
        width: u32,
        names: &[&str],
    ) where
        V::Widget: Sized,
    {
        let mut ui = Ui::new(state, build, (width, 54));
        assert!(ids(&ui.harness).iter().any(|id| {
            *ui.harness.get_widget_with_id(*id).get_prop::<Background>()
                == Background::Color(components::CHROME_RAISED)
        }));
        for name in names {
            let button = ui.button(name);
            let before = ui.harness.access_node(button).unwrap().bounding_box();
            assert_eq!(
                *ui.harness
                    .get_widget_with_id(button)
                    .get_prop::<Background>(),
                Background::Color(xilem::Color::TRANSPARENT),
                "{name} must leave the backdrop visible"
            );
            ui.harness.focus_on(Some(button));
            assert_eq!(
                ui.harness
                    .get_widget_with_id(button)
                    .get_prop::<BorderColor>()
                    .color,
                components::TEXT,
                "{name} needs contrast over the darkest tinted backdrop"
            );
            assert_eq!(
                before,
                ui.harness.access_node(button).unwrap().bounding_box(),
                "{name} must not move on focus"
            );
            ui.harness
                .process_text_event(TextEvent::WindowFocusChange(false));
            assert_eq!(ui.harness.focused_widget_id(), None, "{name}");
            assert_ne!(
                ui.harness
                    .get_widget_with_id(button)
                    .get_prop::<BorderColor>()
                    .color,
                components::TEXT,
                "{name} must not keep a ring in an inactive corner"
            );
            ui.harness
                .process_text_event(TextEvent::WindowFocusChange(true));
        }
    }
    check(state(), notifications, 108, &["Notifications"]);
    check(state(), canvas, 102, &["Canvas map", "Select window group"]);
    check(state(), system, 300, &["Control center", "Power"]);
    let mut dock = state();
    dock.catalog = Default::default();
    dock.windows = vec![super::super::window_catalog::RunningWindow {
        id: 1,
        app_id: "editor".into(),
        title: "Document".into(),
        focused: false,
    }];
    check(dock, apps, 106, &["Switch to editor", "Apps List"]);

    let mut popup = Ui::new(
        state(),
        |state| {
            sized_box(super::super::notification_view::panel(
                &state.outputs[0],
                vec![],
                None,
            ))
        },
        (300, 200),
    );
    let close = popup.button("Close notifications");
    popup.harness.focus_on(Some(close));
    assert_eq!(
        popup
            .harness
            .get_widget_with_id(close)
            .get_prop::<BorderColor>()
            .color,
        components::TEXT,
        "opaque popup controls retain the native ink focus style"
    );
    popup
        .harness
        .process_text_event(TextEvent::WindowFocusChange(false));
    assert_eq!(
        popup.harness.focused_widget_id(),
        Some(close),
        "ordinary popups retain their focus position"
    );
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
    let mut ui = Ui::new(state, system, (236, 54));
    for name in ["Application tray", "Control center", "Power"] {
        let button = ui.button(name);
        let bounds = ui
            .harness
            .access_node(button)
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(
            bounds.x0 >= 0.0 && bounds.x1 <= 236.0 && bounds.y1 <= 54.0,
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
    let ui = Ui::new(state(), notifications, (128, 54));
    let bounds = ui
        .harness
        .access_node(ui.button("Notifications"))
        .unwrap()
        .bounding_box()
        .unwrap();
    let status = ids(&ui.harness)
        .into_iter()
        .find_map(|id| {
            let node = ui.harness.access_node(id)?;
            (node.value().as_deref() == Some("All clear"))
                .then(|| node.bounding_box())
                .flatten()
        })
        .unwrap();
    assert!(((bounds.x0 + status.x1) / 2.0 - 64.0).abs() < 1.0);

    let ui = Ui::new(state(), canvas, (108, 54));
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
}

#[test]
fn system_corner_requests_only_its_content_width_and_updates_for_tray_items() {
    let mut state = state();
    state.clock = "Sat Oct 17 02:38".into();
    let mut ui = Ui::new(state, system, (290, 54));
    let empty_width = system_width(&ui.state, &ui.state.outputs[0]);
    assert!(empty_width == 290.0, "spare clock width: {empty_width}");
    ui.state.tray.snapshot.items = vec![crate::ui::tray_tests::item(1)];
    ui.rebuild(&system);
    ui.dispatch(&system);
    let populated_width = system_width(&ui.state, &ui.state.outputs[0]);
    assert!(
        (populated_width - empty_width - 76.0).abs() <= 1.0,
        "one icon + drawer + intervening gaps: {empty_width} -> {populated_width}"
    );
    ui.harness
        .process_window_event(masonry::core::WindowEvent::Resize(
            (populated_width as u32, 54).into(),
        ));
    ui.dispatch(&system);
    ui.button("App 1");
    ui.state.tray.snapshot.items.clear();
    ui.rebuild(&system);
    ui.dispatch(&system);
    assert_eq!(system_width(&ui.state, &ui.state.outputs[0]), empty_width);
}

#[test]
fn hover_feedback_preserves_system_width_and_tray_button_identity() {
    let mut state = state();
    state.tray.snapshot.items = vec![crate::ui::tray_tests::item(1)];
    let mut ui = Ui::new(state, system, (370, 54));
    let width = system_width(&ui.state, &ui.state.outputs[0]);
    ui.harness
        .process_window_event(masonry::core::WindowEvent::Resize(
            (width as u32, 54).into(),
        ));
    ui.dispatch(&system);
    let button = ui.button("App 1");
    let bounds = ui.harness.access_node(button).unwrap().bounding_box();
    ui.state.tray.hovered = Some((
        ui.state.tray.snapshot.items[0].key.clone(),
        "Synchronization status with a very long application description\nA second detail line"
            .into(),
    ));
    ui.rebuild(&system);
    ui.dispatch(&system);
    assert_eq!(
        system_width(&ui.state, &ui.state.outputs[0]),
        width,
        "hover must not request a resize that removes the pointer target before configure"
    );
    assert_eq!(ui.button("App 1"), button);
    assert_eq!(
        ui.harness.access_node(button).unwrap().bounding_box(),
        bounds
    );
    ui.state.clock = "A permanently longer clock presentation after a preference change".into();
    ui.state.tray.hovered = None;
    ui.rebuild(&system);
    ui.dispatch(&system);
    assert_eq!(
        system_width(&ui.state, &ui.state.outputs[0]),
        width,
        "the compact clock slot must stay stable for long localized dates"
    );
}

#[test]
fn multiline_tray_tooltip_does_not_move_or_remove_a_pointer_activation_target() {
    let mut state = state();
    let mut item = crate::ui::tray_tests::item(1);
    item.item_is_menu = true;
    item.tooltip =
        "Synchronization status with a long description\nMore detail for assistive technology"
            .into();
    let description = item.tooltip.clone();
    let key = item.key.clone();
    state.tray.snapshot.items = vec![item];
    let mut ui = Ui::new(state, system, (370, 54));
    let width = system_width(&ui.state, &ui.state.outputs[0]);
    ui.harness
        .process_window_event(masonry::core::WindowEvent::Resize(
            (width as u32, 54).into(),
        ));
    ui.dispatch(&system);
    let button = ui.button("App 1");
    let bounds = ui
        .harness
        .access_node(button)
        .unwrap()
        .bounding_box()
        .unwrap();
    ui.harness.mouse_move_to(button);
    ui.dispatch(&system);
    assert_eq!(ui.button("App 1"), button);
    let node = ui.harness.access_node(button).unwrap();
    assert_eq!(node.bounding_box(), Some(bounds));
    assert_eq!(node.description().as_deref(), Some(description.as_str()));
    assert_eq!(system_width(&ui.state, &ui.state.outputs[0]), width);
    ui.harness
        .mouse_button_press(Some(masonry::core::PointerButton::Primary));
    ui.dispatch(&system);
    ui.harness
        .mouse_button_release(Some(masonry::core::PointerButton::Primary));
    ui.dispatch(&system);
    assert!(ui.state.outputs[0].tray.open);
    assert_eq!(ui.state.outputs[0].tray.item.as_ref(), Some(&key));
}
