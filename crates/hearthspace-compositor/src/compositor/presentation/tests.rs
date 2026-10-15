use super::*;

#[test]
fn redundant_gtk_restore_requests_preserve_the_touch_sequence() {
    use smithay::backend::input::TouchSlot;

    let state = WindowPresentation::default();
    let mut touch = super::super::input::TouchTracker::default();
    let slot = TouchSlot::from(Some(0));
    touch.down("screen", slot, (100.0, 100.0).into());
    touch.motion("screen", slot, (120.0, 100.0).into());
    assert_eq!(touch.frame("screen").len(), 3, "hover, press and motion");

    // GtkWindowHandle sends unset_fullscreen before xdg_toplevel.move, even
    // for a floating window. GDK's layout can also repeat unset_maximized.
    for (fullscreen, maximized) in [(Some(false), None), (None, Some(false))] {
        if state.changes_for_request(fullscreen, maximized, "eDP-1") {
            assert!(
                touch.interrupt().is_empty(),
                "restore released the touch grab"
            );
        }
        touch.motion("screen", slot, (160.0, 120.0).into());
        assert_eq!(
            touch.frame("screen").len(),
            1,
            "held touch must keep moving"
        );
    }
    touch.up("screen", slot);
    assert_eq!(
        touch.frame("screen").len(),
        1,
        "finger lift releases the grab"
    );
}

#[test]
fn changed_window_state_or_output_still_interrupts_input() {
    let mut state = WindowPresentation::default();
    assert!(state.changes_for_request(Some(true), None, "eDP-1"));
    assert!(state.changes_for_request(None, Some(true), "eDP-1"));

    state.update(Some(true), None, (600, 400).into());
    state.output = Some("eDP-1".into());
    assert!(!state.changes_for_request(Some(true), None, "eDP-1"));
    assert!(state.changes_for_request(Some(true), None, "DP-1"));
    assert!(state.changes_for_request(Some(false), None, "eDP-1"));

    state.update(Some(false), Some(true), (1280, 800).into());
    assert!(!state.changes_for_request(Some(false), None, "eDP-1"));
    assert!(!state.changes_for_request(None, Some(true), "eDP-1"));
    assert!(state.changes_for_request(None, Some(false), "eDP-1"));
    assert!(state.changes_for_request(None, Some(true), "DP-1"));
}

#[test]
fn fullscreen_retains_maximized_restore_state() {
    let mut state = WindowPresentation::default();
    let original = Size::from((560, 360));
    state.update(None, Some(true), original);
    state.update(Some(true), None, Size::from((1280, 752)));
    state.update(Some(false), None, Size::from((1280, 800)));
    assert!(state.maximized);
    assert!(state.pinned());
    assert_eq!(state.restore_size, Some(original));
    state.update(None, Some(false), Size::from((1280, 752)));
    assert!(!state.pinned());
    assert!(state.restoring);
    assert_eq!(state.restore_size, Some(original));
}

#[test]
fn rapid_reentry_before_restore_commit_preserves_floating_size() {
    let mut state = WindowPresentation::default();
    let original = Size::from((600, 400));
    state.update(Some(true), None, original);
    state.update(Some(false), None, Size::from((1280, 800)));
    state.update(Some(true), None, Size::from((1280, 800)));
    assert_eq!(state.restore_size, Some(original));
    assert!(!state.restoring);
    state.update(None, Some(true), Size::from((1280, 800)));
    state.update(Some(false), None, Size::from((1280, 800)));
    assert!(state.maximized);
    assert_eq!(state.restore_size, Some(original));
}
