//! Stale menu UI must never invoke a replacement menu's reused IDs.
use super::*;

fn opened() -> ShellState {
    let mut state = state();
    tray_state::open_menu(&mut state, "eDP-1", &key(1));
    state.outputs[0].tray.error = None;
    state
}

fn replace_menu(state: &mut ShellState) {
    state.tray.snapshot.items[0].menu_path = Some("/NewMenu".into());
    tray_state::open_menu(state, "eDP-1", &key(1));
    state.outputs[0].tray.error = None;
}

#[test]
fn queued_menu_action_rejects_a_replacement_menu_with_reused_ids() {
    let mut ui = Ui::new(opened(), panel, (340, 220));
    ui.activate(name(&ui, "First"));
    replace_menu(&mut ui.state);
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert!(
        ui.state.outputs[0].tray.open,
        "stale click must not close the new menu"
    );
    assert!(
        ui.state.outputs[0].tray.error.is_none(),
        "stale click must not enqueue an application command"
    );
}

#[test]
fn held_pointer_rejects_a_replacement_menu_with_reused_ids() {
    let mut ui = Ui::new(opened(), panel, (340, 220));
    ui.harness.mouse_move_to(name(&ui, "First"));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.dispatch(&panel);
    replace_menu(&mut ui.state);
    ui.rebuild(&panel);
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&panel);
    assert!(ui.state.outputs[0].tray.open);
    assert!(ui.state.outputs[0].tray.error.is_none());
}

#[test]
fn replacing_or_removing_menu_path_closes_the_old_menu() {
    for replacement in [None, Some("/NewMenu".into())] {
        let mut state = opened();
        state.tray.snapshot.items[0].menu_path = replacement;
        tray_state::reconcile(&mut state);
        assert!(!state.outputs[0].tray.open);
        assert!(state.outputs[0].tray.menu_path.is_none());
    }
}
