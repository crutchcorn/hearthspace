use super::super::*;

#[test]
fn startup_waits_for_frame_parts_without_creating_legacy_corner_windows() {
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    let background_windows = 1 // the output watcher
        + crate::ui::desktop::windows(&state).len()
        + crate::ui::canvas_search::windows(&state).len()
        + crate::ui::window_switcher::window(&state).into_iter().count();
    assert_eq!(
        crate::ui::corners::windows(&state, &state.outputs[0]).len(),
        4
    );
    assert_eq!(
        crate::ui::views::app_logic(&mut state).count(),
        background_windows
    );

    let snapshot = Snapshot {
        version: 1,
        outputs: vec![Output {
            output: "test".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    apply(&mut state, snapshot.clone());
    let frame_windows = hearthspace_common::frame_layout::WIDGETS.len();
    assert_eq!(windows(&state).len(), frame_windows);
    assert_eq!(
        crate::ui::views::app_logic(&mut state).count(),
        background_windows + frame_windows
    );
    let identities = state.frame_layout.windows.clone();

    // An unavailable/locked snapshot stashes the same native parts. It never
    // creates fallback corners or exposes a stale private editor.
    apply(&mut state, Snapshot::default());
    assert!(!state.frame_layout.visible);
    assert_eq!(
        crate::ui::views::app_logic(&mut state).count(),
        background_windows + frame_windows
    );
    assert_eq!(state.frame_layout.windows, identities);
    apply(&mut state, snapshot);
    assert!(state.frame_layout.visible);
    assert_eq!(state.frame_layout.windows, identities);
}
