//! Exercise the native menu and safe confirmation without a live power service.
use super::{
    test_support::{click, key, touch},
    *,
};
use crate::ui::test_support::{Ui, ids};
use hearthspace_ui_components::{self as components, Palette};
use masonry::{
    accesskit::Role,
    core::{
        TextEvent,
        keyboard::{Key, NamedKey},
    },
    properties::{BorderColor, ContentColor},
};
use xilem::view::sized_box;

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(power_panel(state, &state.outputs[0]))
}

fn state() -> ShellState {
    let mut state = ShellState::new("/unused-power-test".into(), vec!["eDP-1".into()]);
    state.toggle_power("eDP-1");
    state
}

#[test]
fn compact_rows_keep_touch_targets_and_arrows_skip_unavailable_suspend() {
    let mut state = state();
    state.power.status.available = true;
    let mut ui = Ui::new(state, view, (224, 158));
    let lock = ui.button("Lock screen");
    let logout = ui.button("Log off");
    assert_eq!(ui.harness.focused_widget_id(), Some(lock));
    for name in ["Lock screen", "Suspend", "Log off", "Reboot", "Turn off"] {
        let bounds = ui
            .harness
            .access_node(ui.button(name))
            .unwrap()
            .bounding_box()
            .unwrap();
        assert_eq!(bounds.height(), 44.0, "{name}: {bounds:?}");
        assert!(bounds.x0 >= 7.0 && bounds.x1 <= 217.0, "{name}: {bounds:?}");
    }
    assert!(
        ui.harness
            .access_node(ui.button("Suspend"))
            .unwrap()
            .is_disabled()
    );
    key(&mut ui, NamedKey::ArrowDown, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(logout));
    key(&mut ui, NamedKey::ArrowDown, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(lock));
    key(&mut ui, NamedKey::End, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(logout));
    key(&mut ui, NamedKey::Home, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(lock));
    key(&mut ui, NamedKey::ArrowUp, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(logout));
    assert_eq!(
        ui.harness
            .get_widget_with_id(logout)
            .get_prop::<BorderColor>()
            .color,
        components::FOCUS_RING_ON_GLASS,
    );
    key(&mut ui, NamedKey::Escape, &view);
    assert!(!ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].launch_error.is_none());
}

#[test]
fn capability_update_recovers_menu_focus_and_keeps_keyboard_navigation_working() {
    let mut state = state();
    let mut commands = crate::ui::power_task::test_channel(&mut state);
    state.power.status.available = true;
    state.power.status.reboot_available = true;
    let mut ui = Ui::new(state, view, (224, 158));
    let lock = ui.button("Lock screen");
    let reboot = ui.button("Reboot");
    key(&mut ui, NamedKey::End, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(reboot));
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(reboot));

    ui.state.power.status.reboot_available = false;
    ui.state.power.status.reboot_error = Some("Reboot is no longer available.".into());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(ui.harness.access_node(reboot).unwrap().is_disabled());
    assert_eq!(ui.harness.focused_widget_id(), Some(lock));
    assert_eq!(
        ui.harness
            .get_widget_with_id(lock)
            .get_prop::<BorderColor>()
            .color,
        components::FOCUS_RING_ON_GLASS,
    );
    assert!(ui.state.outputs[0].power_confirmation.is_none());
    key(&mut ui, NamedKey::ArrowDown, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Suspend")));
    key(&mut ui, NamedKey::End, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Log off")));
    key(&mut ui, NamedKey::Home, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(lock));
    key(&mut ui, NamedKey::End, &view);
    key(&mut ui, NamedKey::Enter, &view);
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::LogOff)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Cancel")));
    key(&mut ui, NamedKey::Escape, &view);
    assert!(!ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
    assert!(ui.state.outputs[0].launch_error.is_none());
    assert!(commands.try_recv().is_err());
}

fn held_activation_release_does_not_transfer_to_lock(activation: Key) {
    use std::os::unix::net::UnixListener;

    let dir = tempfile::tempdir().unwrap();
    let mut state = state();
    state.command_socket = dir.path().join("control.sock");
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut commands = crate::ui::power_task::test_channel(&mut state);
    state.power.status.available = true;
    state.power.status.reboot_available = true;
    let mut ui = Ui::new(state, view, (224, 158));
    key(&mut ui, NamedKey::End, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Reboot")));

    ui.harness
        .process_text_event(TextEvent::key_down(activation.clone()));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
    ui.state.power.status.reboot_available = false;
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Lock screen"))
    );
    assert!(
        ui.harness
            .access_node(ui.button("Reboot"))
            .unwrap()
            .is_disabled()
    );
    ui.harness
        .process_text_event(TextEvent::key_up(activation.clone()));
    ui.dispatch(&view);

    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "Releasing the key pressed on Reboot must not send lock-session"
    );
    assert!(ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
    assert!(ui.state.outputs[0].launch_error.is_none());
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Lock screen"))
    );
    assert!(commands.try_recv().is_err());

    // The interrupted press must not prevent a fresh, deliberate activation.
    ui.state.power.status.reboot_available = true;
    ui.rebuild(&view);
    ui.dispatch(&view);
    key(&mut ui, NamedKey::End, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Reboot")));
    ui.harness
        .process_text_event(TextEvent::key_down(activation.clone()));
    ui.dispatch(&view);
    ui.harness.process_text_event(TextEvent::key_up(activation));
    ui.dispatch(&view);
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::Reboot)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Cancel")));
    assert!(commands.try_recv().is_err());
}

#[test]
fn held_enter_release_after_capability_loss_does_not_lock() {
    held_activation_release_does_not_transfer_to_lock(Key::Named(NamedKey::Enter));
}

#[test]
fn held_space_release_after_capability_loss_does_not_lock() {
    held_activation_release_does_not_transfer_to_lock(Key::Character(" ".into()));
}

#[test]
fn pointer_opens_a_named_dialog_and_enter_on_initial_cancel_keeps_session_open() {
    let mut ui = Ui::new(state(), view, (420, 230));
    let logout = ui.button("Log off");
    click(&mut ui, logout, &view);
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::LogOff)
    );
    let cancel = ui.button("Cancel");
    assert_eq!(ui.harness.focused_widget_id(), Some(cancel));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.role() == Role::Dialog && node.label().as_deref() == Some("Log off?")
        })
    }));
    assert_eq!(
        ui.harness
            .get_widget_with_id(cancel)
            .get_prop::<BorderColor>()
            .color,
        components::FOCUS_RING_ON_GLASS,
        "Cancel must have a visible keyboard focus indicator",
    );
    key(&mut ui, NamedKey::Enter, &view);
    assert!(!ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
    assert!(
        ui.state.outputs[0].launch_error.is_none(),
        "Cancel must not contact the missing service"
    );
}

#[test]
fn affirmative_logout_preserves_real_command_and_keeps_rejection_visible() {
    use std::{
        io::{Read, Write},
        os::unix::net::UnixListener,
        thread,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut state = state();
    state.command_socket = dir.path().join("control.sock");
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut ui = Ui::new(state, view, (420, 230));
    key(&mut ui, NamedKey::End, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Log off")));
    key(&mut ui, NamedKey::Enter, &view);
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::LogOff)
    );
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Cancel")));
    listener.set_nonblocking(false).unwrap();
    let service = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = String::new();
        socket.read_to_string(&mut request).unwrap();
        assert_eq!(request, "quit\n");
        socket.write_all(b"err session unavailable\n").unwrap();
    });
    ui.activate(ui.button("Log off"));
    ui.dispatch(&view);
    service.join().unwrap();
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::LogOff)
    );
    assert!(ui.state.outputs[0].show_power);
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness
            .access_node(id)
            .is_some_and(|node| node.value().as_deref() == Some("session unavailable"))
    }));
    ui.button("Cancel");
    key(&mut ui, NamedKey::Escape, &view);
    assert!(!ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
}

#[test]
fn changing_to_dark_mode_keeps_focus_and_uses_dark_menu_and_dialog_ink() {
    let mut ui = Ui::new(state(), view, (420, 230));
    let logout = ui.button("Log off");
    ui.harness.focus_on(Some(logout));
    let palette = Palette::new(hearthspace_common::Appearance {
        color_mode: hearthspace_common::ColorMode::Dark,
        ..Default::default()
    });
    ui.state.appearance.update(palette);
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(logout));
    assert_eq!(
        ui.harness
            .get_widget_with_id(logout)
            .get_prop::<BorderColor>()
            .color,
        palette.focus_ring_on_glass
    );
    let label = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .access_node(*id)
                .is_some_and(|node| node.value().as_deref() == Some("Lock screen"))
        })
        .unwrap();
    assert_eq!(
        ui.harness
            .get_widget_with_id(label)
            .get_prop::<ContentColor>()
            .color,
        palette.for_glass().accent_strong
    );
    key(&mut ui, NamedKey::Enter, &view);
    let cancel = ui.button("Cancel");
    assert_eq!(ui.harness.focused_widget_id(), Some(cancel));
    assert_eq!(
        ui.harness
            .get_widget_with_id(cancel)
            .get_prop::<BorderColor>()
            .color,
        palette.focus_ring_on_glass
    );
}

#[test]
fn touch_scroll_does_not_activate_lock_and_touch_confirmation_can_be_cancelled() {
    let mut state = state();
    state.outputs[0].launch_error = Some("A detailed session service error. ".repeat(12));
    let mut ui = Ui::new(state, view, (224, 180));
    let lock = ui.button("Lock screen");
    ui.harness.scroll_into_view(lock);
    touch(&mut ui, lock, 50.0, &view);
    assert!(
        ui.state.outputs[0]
            .launch_error
            .as_deref()
            .unwrap()
            .starts_with("A detailed")
    );
    assert!(ui.state.outputs[0].power_confirmation.is_none());
    let logout = ui.button("Log off");
    ui.harness.scroll_into_view(logout);
    touch(&mut ui, logout, 0.0, &view);
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::LogOff)
    );
    let cancel = ui.button("Cancel");
    ui.harness.scroll_into_view(cancel);
    touch(&mut ui, cancel, 0.0, &view);
    assert!(!ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
    assert!(ui.state.outputs[0].launch_error.is_none());
}
