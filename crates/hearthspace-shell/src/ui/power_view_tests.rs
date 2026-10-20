//! The restyled menu keeps session actions accessible in short output palettes.
use super::*;
use crate::ui::test_support::{Ui, ids};
use hearthspace_ui_components as components;
use masonry::{
    core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    },
    properties::BorderColor,
};
use xilem::view::sized_box;

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(power_panel(&state.outputs[0], &state.power))
}

#[test]
fn power_actions_have_roomy_targets_and_keyboard_confirmation_keeps_session_open() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_power("eDP-1");
    let mut ui = Ui::new(state, view, (320, 400));
    for name in ["Lock", "Suspend", "Log out"] {
        let bounds = ui
            .harness
            .access_node(ui.button(name))
            .unwrap()
            .bounding_box()
            .unwrap();
        assert!(bounds.height() >= 44.0, "{name}: {bounds:?}");
        assert!(
            bounds.x0 >= 20.0 && bounds.x1 <= 300.0,
            "{name}: {bounds:?}"
        );
    }
    let logout = ui.button("Log out");
    ui.harness.focus_on(Some(logout));
    assert_eq!(
        ui.harness
            .get_widget_with_id(logout)
            .get_prop::<BorderColor>()
            .color,
        components::FOCUS_RING_ON_GLASS,
        "the action must retain the shared visible keyboard focus outline"
    );
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].confirm_logout);
    ui.button("Confirm log out");
    ui.harness.mouse_click_on(ui.button("Cancel"), None);
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].show_power);
    assert!(!ui.state.outputs[0].confirm_logout);
}

#[test]
fn power_header_stays_reachable_when_real_errors_need_body_scrolling() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.toggle_power("eDP-1");
    let error = "The session could not be locked because the lock service did not respond. Try again when the session service is available.";
    state.outputs[0].launch_error = Some(error.into());
    let mut ui = Ui::new(state, view, (280, 180));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.value().as_deref() == Some(error))
    }));
    let close = ui.button("Close power menu");
    let before = ui
        .harness
        .access_node(close)
        .unwrap()
        .bounding_box()
        .unwrap();
    ui.harness.scroll_into_view(ui.button("Log out"));
    assert_eq!(
        ui.harness
            .access_node(close)
            .unwrap()
            .bounding_box()
            .unwrap(),
        before
    );
    ui.harness.mouse_click_on(ui.button("Log out"), None);
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].confirm_logout);
    ui.harness.scroll_into_view(ui.button("Cancel"));
    ui.harness.mouse_click_on(ui.button("Cancel"), None);
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].confirm_logout);
    ui.harness
        .mouse_click_on(ui.button("Close power menu"), None);
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].show_power);
}
