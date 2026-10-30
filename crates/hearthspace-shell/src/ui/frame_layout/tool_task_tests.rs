use super::*;
use hearthspace_ipc::command::CanvasTool;

#[path = "tool_selection_tests.rs"]
mod selection;

#[test]
fn unavailable_worker_clears_selection_and_identical_confirmation_recovers() {
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    let snapshot = Snapshot {
        version: 1,
        canvas_tool: Some(CanvasTool::Pan),
        outputs: vec![super::super::Output {
            output: "test".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    apply_update(&mut state, Update::Snapshot(Box::new(snapshot.clone())));
    assert_eq!(
        state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Pan)
    );
    apply_update(&mut state, Update::Error("disconnected".into()));
    assert_eq!(state.frame_layout.snapshot.canvas_tool, None);
    assert_eq!(super::super::tool_menu::primary(&state), CanvasTool::Pan);
    assert!(state.frame_layout.error.is_some());
    apply_update(&mut state, Update::Snapshot(Box::new(snapshot)));
    assert_eq!(
        state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Pan)
    );
    assert!(state.frame_layout.error.is_none());
}
