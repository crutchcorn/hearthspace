use super::*;
use hearthspace_ipc::command::CanvasTool;

#[test]
fn acknowledged_selection_waits_for_snapshot_and_recovers_without_replaying_command() {
    let server = Server::new(vec![
        Reply::Ack,
        Reply::Timeout,
        Reply::Tool(CanvasTool::Pan),
    ]);
    let (mut state, mut requests) = setup(&server.path());
    super::super::super::select_tool(&mut state, CanvasTool::Pan);
    assert_eq!(state.frame_layout.snapshot.canvas_tool, None);
    let mut worker = Worker::default();
    assert!(
        worker
            .poll(
                &server.path(),
                Some(requests.try_recv().unwrap()),
                vec![],
                1
            )
            .is_err()
    );
    assert_eq!(
        state.frame_layout.snapshot.canvas_tool, None,
        "an acknowledgement is not an observed selection"
    );
    server.release.send(()).unwrap();
    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    assert_eq!(
        state.frame_layout.snapshot.canvas_tool, None,
        "worker completion still waits for UI delivery"
    );
    super::super::super::apply(&mut state, snapshot);
    assert_eq!(
        state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Pan)
    );
    assert_eq!(
        server.task.join().unwrap(),
        ["canvas-tool pan\n", "layout-status\n", "layout-status\n"]
    );
}

#[test]
fn rejected_selection_never_becomes_selected_and_is_not_retried() {
    let server = Server::new(vec![Reply::Reject, Reply::Tool(CanvasTool::Select)]);
    let (mut state, mut requests) = setup(&server.path());
    super::super::super::select_tool(&mut state, CanvasTool::Pan);
    let mut worker = Worker::default();
    assert!(
        worker
            .poll(
                &server.path(),
                Some(requests.try_recv().unwrap()),
                vec![],
                1
            )
            .is_err()
    );
    assert_ne!(
        state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Pan)
    );
    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    super::super::super::apply(&mut state, snapshot);
    assert_eq!(
        state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Select)
    );
    assert_eq!(
        server.task.join().unwrap(),
        ["canvas-tool pan\n", "layout-status\n"]
    );
}

#[test]
fn rapid_explicit_choices_keep_queue_order_and_never_toggle_on_duplicate_selection() {
    let server = Server::new(vec![
        Reply::Ack,
        Reply::Tool(CanvasTool::Pan),
        Reply::Ack,
        Reply::Tool(CanvasTool::Search),
        Reply::Ack,
        Reply::Tool(CanvasTool::Search),
    ]);
    let (mut state, mut requests) = setup(&server.path());
    for tool in [CanvasTool::Pan, CanvasTool::Search, CanvasTool::Search] {
        super::super::super::select_tool(&mut state, tool);
    }
    assert_eq!(state.frame_layout.snapshot.canvas_tool, None);
    let mut worker = Worker::default();
    for (id, tool) in [
        (1, CanvasTool::Pan),
        (2, CanvasTool::Search),
        (3, CanvasTool::Search),
    ] {
        let snapshot = worker
            .poll(
                &server.path(),
                Some(requests.try_recv().unwrap()),
                vec![],
                id,
            )
            .unwrap();
        super::super::super::apply(&mut state, snapshot);
        assert_eq!(state.frame_layout.snapshot.canvas_tool, Some(tool));
    }
    assert!(requests.try_recv().is_err());
    assert_eq!(
        server.task.join().unwrap(),
        [
            "canvas-tool pan\n",
            "layout-status\n",
            "canvas-tool search\n",
            "layout-status\n",
            "canvas-tool search\n",
            "layout-status\n"
        ]
    );
}
