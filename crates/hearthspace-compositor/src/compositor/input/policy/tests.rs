use super::*;

#[test]
fn client_takeover_continues_an_owned_press_while_new_navigation_is_blocked() {
    let mut state = Policy {
        pointer_grab: true,
        ..Default::default()
    };
    assert!(!state.allows(Action::Navigation, Source::Local, Phase::Begin));
    assert!(state.allows(
        Action::ClientWindowOperation,
        Source::Client,
        Phase::Continue
    ));
    assert!(!state.allows(Action::ClientWindowOperation, Source::Client, Phase::Begin));
    assert!(!state.allows(
        Action::ClientWindowOperation,
        Source::Remote,
        Phase::Continue
    ));
    state.window_operation = true;
    assert!(!state.allows(
        Action::ClientWindowOperation,
        Source::Client,
        Phase::Continue
    ));
    state.window_operation = false;
    state.locked = true;
    assert!(!state.allows(
        Action::ClientWindowOperation,
        Source::Client,
        Phase::Continue
    ));
}

#[test]
fn focused_view_allows_window_switching_but_not_canvas_motion() {
    let state = Policy {
        focused_view: true,
        switcher: true,
        ..Default::default()
    };
    assert!(state.allows(Action::SwitcherGesture, Source::Touch, Phase::Continue));
    assert!(!state.allows(Action::CanvasGesture, Source::Touch, Phase::Continue));
    assert!(!state.allows(Action::Navigation, Source::Local, Phase::Begin));
}

#[test]
fn remote_admission_uses_the_same_modes_after_opt_in_and_seat_activation() {
    let mut state = Policy::default();
    assert!(state.allows(Action::Input, Source::Local, Phase::Begin));
    assert!(!state.allows(Action::Input, Source::Remote, Phase::Begin));
    state.remote_enabled = true;
    state.remote_active = true;
    assert!(state.allows(Action::Input, Source::Remote, Phase::Begin));
    for confirmation in [false, true] {
        state.confirmation = confirmation;
        assert_eq!(
            state.allows(Action::Navigation, Source::Local, Phase::Begin),
            state.allows(Action::Navigation, Source::Remote, Phase::Begin)
        );
    }
    state.locked = true;
    assert!(!state.allows(Action::Input, Source::Remote, Phase::Begin));
}

#[test]
fn extra_contact_cancels_preview_taps_without_cancelling_the_gesture_switcher() {
    let touch = Action::TouchRecognition {
        group_suppressed: false,
    };
    let mut state = Policy {
        switcher: true,
        ..Default::default()
    };
    assert!(state.allows(touch, Source::Touch, Phase::Begin));
    assert!(state.allows(touch, Source::Touch, Phase::Continue));
    state.keyboard_switcher = true;
    assert!(!state.allows(touch, Source::Touch, Phase::Continue));
    state.switcher = false;
    state.keyboard_switcher = false;
    state.search = true;
    assert!(state.allows(touch, Source::Touch, Phase::Begin));
    assert!(!state.allows(touch, Source::Touch, Phase::Continue));
    state.search = false;
    state.pointer_grab = true;
    assert!(!state.allows(touch, Source::Touch, Phase::Begin));
    // Locked touches still reach the lock-surface route, never the remote route.
    state.locked = true;
    assert!(state.allows(touch, Source::Touch, Phase::Begin));
}
