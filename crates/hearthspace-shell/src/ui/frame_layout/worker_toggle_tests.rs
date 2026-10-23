use super::*;

#[test]
fn two_toggles_during_query_timeout_preserve_closed_state_without_another_anchor() {
    let server = Server::new(vec![Reply::Ack, Reply::Timeout, Reply::Snapshot]);
    let (mut state, request) = request(&server.path());
    let mut worker = Worker::default();
    assert!(
        worker
            .poll(&server.path(), Some(request), vec![], 1)
            .is_err()
    );
    state.toggle_notifications("eDP-1");
    server.release.send(()).unwrap();
    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    assert!(worker.take_ready(&snapshot, Instant::now()).is_none());
    assert!(!state.outputs[0].show_notifications);
    let commands = server.task.join().unwrap();
    assert_eq!(commands.len(), 3);
    assert_eq!(&commands[1..], ["layout-status\n", "layout-status\n"]);
}

#[test]
fn second_toggle_revokes_an_already_queued_open() {
    let server = Server::new(vec![Reply::Ack, Reply::Snapshot]);
    let (mut state, request) = request(&server.path());
    let mut worker = Worker::default();
    let snapshot = worker
        .poll(&server.path(), Some(request), vec![], 1)
        .unwrap();
    let queued = worker.take_ready(&snapshot, Instant::now()).unwrap();
    state.toggle_notifications("eDP-1");
    queued.deliver(&mut state, Instant::now());
    assert!(!state.outputs[0].show_notifications);
    assert_eq!(server.task.join().unwrap().len(), 2);
}

#[test]
fn two_toggles_from_open_close_immediately_then_recover_the_reopening() {
    let server = Server::new(vec![Reply::Ack, Reply::Timeout, Reply::Snapshot]);
    let (mut state, mut requests) = setup(&server.path());
    state.outputs[0].show_notifications = true;
    state.toggle_notifications("eDP-1");
    assert!(!state.outputs[0].show_notifications);
    assert!(
        requests.try_recv().is_err(),
        "closing must not wait for an anchor"
    );
    state.toggle_notifications("eDP-1");
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
    server.release.send(()).unwrap();
    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    worker
        .take_ready(&snapshot, Instant::now())
        .unwrap()
        .deliver(&mut state, Instant::now());
    assert!(state.outputs[0].show_notifications);
    assert!(requests.try_recv().is_err());
    assert_eq!(server.task.join().unwrap().len(), 3);
}
