//! The real input path publishes this epoch even when no panel has mapped.
use super::*;

#[test]
fn outside_press_event_cancels_acknowledged_recovery_after_query_timeout() {
    let server = Server::new(vec![Reply::Ack, Reply::Timeout, Reply::Epoch(1)]);
    let (mut state, request) = request(&server.path());
    let mut worker = Worker::default();
    assert!(
        worker
            .poll(&server.path(), Some(request), vec![], 1)
            .is_err()
    );
    super::super::super::cancel_popups(&mut state, 1);
    server.release.send(()).unwrap();
    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    assert!(worker.take_ready(&snapshot, Instant::now()).is_none());
    assert!(!state.outputs[0].show_notifications);
    let commands = server.task.join().unwrap();
    assert_eq!(commands.len(), 3);
    assert_eq!(&commands[1..], ["layout-status\n", "layout-status\n"]);
}

#[test]
fn outside_press_event_revokes_dispatched_open_before_a_new_snapshot_arrives() {
    let server = Server::new(vec![Reply::Ack, Reply::Snapshot]);
    let (mut state, request) = request(&server.path());
    let mut worker = Worker::default();
    let snapshot = worker
        .poll(&server.path(), Some(request), vec![], 1)
        .unwrap();
    let queued = worker.take_ready(&snapshot, Instant::now()).unwrap();
    super::super::super::cancel_popups(&mut state, 1);
    assert_eq!(state.frame_layout.snapshot.popup_epoch, 0);
    queued.clone().deliver(&mut state, Instant::now());
    // A delayed old observation must not make the cancelled opening current.
    super::super::super::apply(&mut state, snapshot);
    queued.deliver(&mut state, Instant::now());
    assert!(!state.outputs[0].show_notifications);
    assert_eq!(server.task.join().unwrap().len(), 2);
}
