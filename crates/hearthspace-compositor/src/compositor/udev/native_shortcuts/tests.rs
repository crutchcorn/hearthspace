use super::*;

const F1: u32 = 59 + 8;
const F12: u32 = 88 + 8;

fn press(shortcuts: &mut NativeShortcuts, code: u32) -> NativeAction {
    shortcuts.key("keyboard", code, KeyState::Pressed)
}

fn release(shortcuts: &mut NativeShortcuts, code: u32) -> NativeAction {
    shortcuts.key("keyboard", code, KeyState::Released)
}

fn chord() -> NativeShortcuts {
    let mut shortcuts = NativeShortcuts::default();
    assert_eq!(press(&mut shortcuts, LEFT_CTRL), NativeAction::Forward);
    assert_eq!(press(&mut shortcuts, LEFT_ALT), NativeAction::Forward);
    shortcuts
}

#[test]
fn function_keys_map_exactly_including_noncontiguous_f11_and_f12() {
    for (index, linux_code) in (59..=68).chain(87..=88).enumerate() {
        let mut shortcuts = chord();
        let code = linux_code + 8;
        assert_eq!(
            press(&mut shortcuts, code),
            NativeAction::SwitchVt(index as i32 + 1)
        );
        assert_eq!(release(&mut shortcuts, code), NativeAction::Consume);
    }
    for linux_code in [0, 58, 69, 70, 86, 89, 183] {
        assert_eq!(press(&mut chord(), linux_code + 8), NativeAction::Forward);
    }
    assert_eq!(function_vt(0), None);
}

#[test]
fn both_modifiers_are_required() {
    for modifiers in [vec![], vec![LEFT_CTRL], vec![LEFT_ALT]] {
        let mut shortcuts = NativeShortcuts::default();
        for modifier in modifiers {
            press(&mut shortcuts, modifier);
        }
        assert_eq!(press(&mut shortcuts, F1), NativeAction::Forward);
        assert_eq!(release(&mut shortcuts, F1), NativeAction::Forward);
    }
}

#[test]
fn each_physical_modifier_side_can_form_the_chord() {
    for ctrl in [LEFT_CTRL, RIGHT_CTRL] {
        for alt in [LEFT_ALT, RIGHT_ALT] {
            let mut shortcuts = NativeShortcuts::default();
            press(&mut shortcuts, ctrl);
            press(&mut shortcuts, alt);
            assert_eq!(press(&mut shortcuts, F1), NativeAction::SwitchVt(1));
        }
    }
}

#[test]
fn releasing_one_modifier_side_keeps_the_other_held() {
    let mut shortcuts = chord();
    press(&mut shortcuts, RIGHT_CTRL);
    press(&mut shortcuts, RIGHT_ALT);
    release(&mut shortcuts, LEFT_CTRL);
    release(&mut shortcuts, LEFT_ALT);
    assert_eq!(press(&mut shortcuts, F1), NativeAction::SwitchVt(1));
}

#[test]
fn repeats_and_release_stay_consumed_even_when_the_switch_fails() {
    let mut shortcuts = chord();
    assert_eq!(press(&mut shortcuts, F1), NativeAction::SwitchVt(1));
    // The backend's switch outcome never resets suppression: a failed request
    // must not send an orphan function-key release to the focused client.
    assert_eq!(press(&mut shortcuts, F1), NativeAction::Consume);
    assert_eq!(release(&mut shortcuts, LEFT_CTRL), NativeAction::Forward);
    assert_eq!(release(&mut shortcuts, LEFT_ALT), NativeAction::Forward);
    assert_eq!(press(&mut shortcuts, F1), NativeAction::Consume);
    assert_eq!(release(&mut shortcuts, F1), NativeAction::Consume);
    assert_eq!(press(&mut shortcuts, F1), NativeAction::Forward);
    assert_eq!(release(&mut shortcuts, F1), NativeAction::Forward);
}

#[test]
fn pressing_modifiers_after_a_function_key_does_not_retroactively_intercept_it() {
    let mut shortcuts = NativeShortcuts::default();
    assert_eq!(press(&mut shortcuts, F1), NativeAction::Forward);
    press(&mut shortcuts, LEFT_CTRL);
    press(&mut shortcuts, LEFT_ALT);
    assert_eq!(press(&mut shortcuts, F1), NativeAction::Forward);
    assert_eq!(release(&mut shortcuts, F1), NativeAction::Forward);
    assert_eq!(press(&mut shortcuts, F1), NativeAction::SwitchVt(1));
}

#[test]
fn pause_and_activation_clear_stale_modifiers_and_interceptions() {
    let mut shortcuts = chord();
    assert_eq!(press(&mut shortcuts, F12), NativeAction::SwitchVt(12));
    shortcuts.clear();
    assert_eq!(press(&mut shortcuts, F12), NativeAction::Forward);
    release(&mut shortcuts, F12);
    press(&mut shortcuts, LEFT_CTRL);
    press(&mut shortcuts, LEFT_ALT);
    assert_eq!(press(&mut shortcuts, F12), NativeAction::SwitchVt(12));
}

#[test]
fn modifiers_on_multiple_keyboards_are_aggregated_until_each_device_releases() {
    let mut shortcuts = chord();
    shortcuts.key("second", LEFT_CTRL, KeyState::Pressed);
    shortcuts.key("second", LEFT_ALT, KeyState::Pressed);
    release(&mut shortcuts, LEFT_CTRL);
    release(&mut shortcuts, LEFT_ALT);
    assert_eq!(press(&mut shortcuts, F1), NativeAction::SwitchVt(1));
    release(&mut shortcuts, F1);
    shortcuts.key("second", LEFT_CTRL, KeyState::Released);
    assert_eq!(press(&mut shortcuts, F1), NativeAction::Forward);
}

#[test]
fn unplugging_a_keyboard_removes_only_its_keys() {
    let mut shortcuts = chord();
    shortcuts.key("second", LEFT_CTRL, KeyState::Pressed);
    shortcuts.key("second", LEFT_ALT, KeyState::Pressed);
    shortcuts.remove_device("keyboard");
    assert_eq!(press(&mut shortcuts, F1), NativeAction::SwitchVt(1));
    release(&mut shortcuts, F1);
    shortcuts.remove_device("second");
    assert_eq!(press(&mut shortcuts, F1), NativeAction::Forward);
}

#[test]
fn identical_function_keys_on_two_devices_keep_independent_suppression() {
    let mut shortcuts = chord();
    assert_eq!(press(&mut shortcuts, F1), NativeAction::SwitchVt(1));
    assert_eq!(
        shortcuts.key("second", F1, KeyState::Pressed),
        NativeAction::SwitchVt(1)
    );
    assert_eq!(release(&mut shortcuts, F1), NativeAction::Consume);
    assert_eq!(
        shortcuts.key("second", F1, KeyState::Released),
        NativeAction::Consume
    );
}

#[test]
fn removing_device_clears_its_intercepted_key_for_reconnection() {
    let mut shortcuts = chord();
    assert_eq!(press(&mut shortcuts, F1), NativeAction::SwitchVt(1));
    shortcuts.remove_device("keyboard");
    assert_eq!(press(&mut shortcuts, F1), NativeAction::Forward);
    assert_eq!(release(&mut shortcuts, F1), NativeAction::Forward);
}

#[test]
fn emergency_exit_keeps_both_existing_bindings() {
    for code in [ESC, BACKSPACE] {
        assert_eq!(
            press(&mut NativeShortcuts::default(), code),
            NativeAction::Forward
        );
        let mut shortcuts = chord();
        assert_eq!(press(&mut shortcuts, code), NativeAction::Exit);
        assert_eq!(press(&mut shortcuts, code), NativeAction::Consume);
        assert_eq!(release(&mut shortcuts, code), NativeAction::Consume);
    }
}
