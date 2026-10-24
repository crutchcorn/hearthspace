//! Destructive actions stop at the fake worker queue until explicitly confirmed.
use super::{
    test_support::{click, key},
    *,
};
use crate::ui::{
    power_task::{self, Action},
    test_support::{Ui, ids},
};
use masonry::{accesskit::Role, core::keyboard::NamedKey};
use xilem::view::sized_box;

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(power_panel(state, &state.outputs[0]))
}

fn state() -> (ShellState, tokio::sync::mpsc::Receiver<(String, Action)>) {
    let mut state = ShellState::new("/unused-power-confirmation".into(), vec!["eDP-1".into()]);
    state.toggle_power("eDP-1");
    let receiver = power_task::test_channel(&mut state);
    state.power.status.available = true;
    state.power.status.reboot_available = true;
    state.power.status.power_off_available = true;
    (state, receiver)
}

#[test]
fn reboot_opens_its_confirmation_and_initial_enter_cancels_without_queueing() {
    let (state, mut commands) = state();
    let mut ui = Ui::new(state, view, (420, 280));
    let reboot = ui.button("Reboot");
    click(&mut ui, reboot, &view);
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::Reboot)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Cancel")));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.role() == Role::Dialog && node.label().as_deref() == Some("Reboot?")
        })
    }));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.value().as_deref() == Some("Save your work before restarting your computer.")
        })
    }));
    assert!(commands.try_recv().is_err());
    key(&mut ui, NamedKey::Enter, &view);
    assert!(commands.try_recv().is_err());
    assert!(!ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
}

#[test]
fn turn_off_confirmation_queues_exact_action_once_and_disables_pending_affirmative() {
    let (state, mut commands) = state();
    let mut ui = Ui::new(state, view, (342, 280));
    key(&mut ui, NamedKey::End, &view);
    key(&mut ui, NamedKey::Enter, &view);
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::PowerOff)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Cancel")));
    assert!(commands.try_recv().is_err());
    ui.activate(ui.button("Turn off"));
    ui.dispatch(&view);
    assert_eq!(
        commands.try_recv().unwrap(),
        ("eDP-1".to_owned(), Action::PowerOff)
    );
    assert_eq!(ui.state.power.pending_action, Some(Action::PowerOff));
    let pending = ui.button("Turning off…");
    assert!(ui.harness.access_node(pending).unwrap().is_disabled());
    ui.activate(pending);
    ui.dispatch(&view);
    power_confirmation::confirm(&mut ui.state, "eDP-1", Confirmation::PowerOff);
    assert!(commands.try_recv().is_err());
}

#[test]
fn reboot_rechecks_capability_and_rejection_keeps_cancel_available() {
    let (state, mut commands) = state();
    let mut ui = Ui::new(state, view, (342, 220));
    ui.activate(ui.button("Reboot"));
    ui.dispatch(&view);
    ui.state.power.status.reboot_available = false;
    ui.state.power.status.reboot_error = Some("Another application is blocking reboot.".into());
    ui.rebuild(&view);
    ui.dispatch(&view);
    let reboot = ui.button("Reboot");
    assert!(ui.harness.access_node(reboot).unwrap().is_disabled());
    ui.activate(reboot);
    ui.dispatch(&view);
    power_confirmation::confirm(&mut ui.state, "eDP-1", Confirmation::Reboot);
    assert!(commands.try_recv().is_err());
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.value().as_deref() == Some("Another application is blocking reboot.")
        })
    }));
    ui.activate(ui.button("Cancel"));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
}

#[test]
fn reboot_affirmative_routes_only_the_selected_output_and_action() {
    let (state, mut commands) = state();
    let mut ui = Ui::new(state, view, (420, 280));
    ui.activate(ui.button("Reboot"));
    ui.dispatch(&view);
    power_confirmation::confirm(&mut ui.state, "eDP-1", Confirmation::PowerOff);
    power_confirmation::confirm(&mut ui.state, "DP-2", Confirmation::Reboot);
    assert!(commands.try_recv().is_err());
    let cancel = ui.button("Cancel");
    let reboot = ui.button("Reboot");
    key(&mut ui, NamedKey::Tab, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(reboot));
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(reboot));
    click(&mut ui, reboot, &view);
    assert_eq!(
        commands.try_recv().unwrap(),
        ("eDP-1".to_owned(), Action::Reboot)
    );
    assert_eq!(ui.state.power.pending_action, Some(Action::Reboot));
    assert_eq!(ui.button("Close"), cancel);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(cancel),
        "Disabling the pressed affirmative must recover the safe focus target"
    );
    ui.state.power.pending = false;
    ui.state.power.pending_action = None;
    ui.state.finish_launch(
        "eDP-1",
        Err("Reboot was denied by the session service.".into()),
    );
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].show_power);
    assert_eq!(
        ui.state.outputs[0].power_confirmation,
        Some(Confirmation::Reboot)
    );
    assert_eq!(ui.button("Cancel"), cancel);
    assert_eq!(ui.harness.focused_widget_id(), Some(cancel));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.value().as_deref() == Some("Reboot was denied by the session service.")
        })
    }));
    key(&mut ui, NamedKey::Escape, &view);
    assert!(!ui.state.outputs[0].show_power);
    assert!(ui.state.outputs[0].power_confirmation.is_none());
    assert!(commands.try_recv().is_err());
}

#[test]
fn confirmation_focus_recovery_leaves_valid_sibling_focus_alone() {
    use xilem::view::{button, flex_col, label};

    let view = |state: &ShellState| {
        sized_box(flex_col((
            power_panel(state, &state.outputs[0]),
            button(label("Other control"), |_state: &mut ShellState| {}),
        )))
    };
    let (state, mut commands) = state();
    let mut ui = Ui::new(state, view, (420, 320));
    ui.activate(ui.button("Reboot"));
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Cancel")));
    key(&mut ui, NamedKey::Tab, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Reboot")));
    key(&mut ui, NamedKey::Tab, &view);
    let sibling = ui.button("Other control");
    assert_eq!(ui.harness.focused_widget_id(), Some(sibling));
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(sibling));
    assert!(commands.try_recv().is_err());
}

#[test]
fn unavailable_reboot_and_power_off_stay_visible_and_cannot_arm_confirmation() {
    let (mut state, mut commands) = state();
    state.power.status.reboot_available = false;
    state.power.status.power_off_available = false;
    let mut ui = Ui::new(state, view, (224, 280));
    for name in ["Reboot", "Turn off"] {
        let action = ui.button(name);
        assert!(ui.harness.access_node(action).unwrap().is_disabled());
        ui.activate(action);
        ui.dispatch(&view);
        assert!(ui.state.outputs[0].power_confirmation.is_none());
        assert!(commands.try_recv().is_err());
    }
    key(&mut ui, NamedKey::End, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Log off")));
}
