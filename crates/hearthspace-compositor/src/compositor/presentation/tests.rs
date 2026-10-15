use super::*;

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
