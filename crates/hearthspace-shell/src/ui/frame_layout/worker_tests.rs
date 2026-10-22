use super::*;
use crate::ui::state::ShellState;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::mpsc,
    thread,
    time::Duration,
};

enum Reply {
    Ack,
    Reject,
    Timeout,
    Snapshot,
    Epoch(u64),
    Truncated,
    InvalidHeader,
    MalformedSnapshot,
}

struct Server {
    directory: tempfile::TempDir,
    release: mpsc::Sender<()>,
    task: thread::JoinHandle<Vec<String>>,
}

impl Server {
    fn new(replies: Vec<Reply>) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(directory.path().join("control.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        let (release, wait) = mpsc::channel();
        let task = thread::spawn(move || {
            let mut commands = Vec::new();
            for reply in replies {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "missing worker request");
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut command = String::new();
                BufReader::new(&mut stream).read_line(&mut command).unwrap();
                commands.push(command);
                match reply {
                    Reply::Ack => stream.write_all(b"ok\n").unwrap(),
                    Reply::Reject => stream.write_all(b"err rejected\n").unwrap(),
                    Reply::Timeout => wait.recv_timeout(Duration::from_secs(5)).unwrap(),
                    Reply::Truncated => stream.write_all(b"ok 200\n{}").unwrap(),
                    Reply::InvalidHeader => stream.write_all(b"invalid\n").unwrap(),
                    Reply::MalformedSnapshot => stream.write_all(b"ok 1\n{").unwrap(),
                    Reply::Snapshot | Reply::Epoch(_) => {
                        let bytes = match reply {
                            Reply::Epoch(epoch) => serde_json::to_vec(&serde_json::json!({
                                "version":1,"popup_epoch":epoch,"outputs":[{"output":"eDP-1"}]
                            }))
                            .unwrap(),
                            _ => snapshot_bytes(),
                        };
                        writeln!(stream, "ok {}", bytes.len()).unwrap();
                        stream.write_all(&bytes).unwrap();
                    }
                }
            }
            commands
        });
        Self {
            directory,
            release,
            task,
        }
    }

    fn path(&self) -> std::path::PathBuf {
        self.directory.path().join("control.sock")
    }
}

fn snapshot_bytes() -> Vec<u8> {
    br#"{"version":1,"outputs":[{"output":"eDP-1"}]}"#.to_vec()
}

fn request(path: &Path) -> (ShellState, Request) {
    let (mut state, mut receive) = setup(path);
    state.toggle_notifications("eDP-1");
    (state, receive.try_recv().unwrap())
}

fn setup(path: &Path) -> (ShellState, tokio::sync::mpsc::Receiver<Request>) {
    let mut state = ShellState::new(path.to_owned(), vec!["eDP-1".into()]);
    super::super::apply(&mut state, parse(&snapshot_bytes()).unwrap());
    let (send, receive) = tokio::sync::mpsc::channel(4);
    state.frame_layout.requests = Some(send);
    (state, receive)
}

#[path = "worker_outside_tests.rs"]
mod outside;
#[path = "worker_toggle_tests.rs"]
mod toggles;

#[test]
fn acknowledged_popup_recovers_after_real_query_timeout_without_replaying_anchor() {
    let server = Server::new(vec![
        Reply::Ack,
        Reply::Timeout,
        Reply::Snapshot,
        Reply::Snapshot,
    ]);
    let (mut state, request) = request(&server.path());
    let mut worker = Worker::default();
    let started = Instant::now();
    assert!(
        worker
            .poll(&server.path(), Some(request), vec![], 1)
            .is_err()
    );
    assert!(started.elapsed() >= Duration::from_millis(700));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!state.outputs[0].show_notifications);
    server.release.send(()).unwrap();

    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    assert_eq!(
        snapshot, state.frame_layout.snapshot,
        "unchanged snapshots must recover too"
    );
    let opening = worker.take_ready(&snapshot, Instant::now()).unwrap();
    // This is the same UI delivery path as Update::Open.
    opening.clone().deliver(&mut state, Instant::now());
    assert!(state.outputs[0].show_notifications);
    opening.deliver(&mut state, Instant::now());
    assert!(
        state.outputs[0].show_notifications,
        "duplicate delivery must not toggle closed"
    );
    state.dismiss_notifications("eDP-1");
    let snapshot = worker.poll(&server.path(), None, vec![], 3).unwrap();
    assert!(worker.take_ready(&snapshot, Instant::now()).is_none());
    assert!(!state.outputs[0].show_notifications);
    let commands = server.task.join().unwrap();
    assert!(commands[0].starts_with("frame-popup-anchor "));
    assert_eq!(
        &commands[1..],
        ["layout-status\n", "layout-status\n", "layout-status\n"]
    );
}

#[test]
fn rejected_anchor_never_becomes_a_recoverable_opening() {
    let server = Server::new(vec![Reply::Reject, Reply::Snapshot]);
    let (state, request) = request(&server.path());
    let mut worker = Worker::default();
    assert!(
        worker
            .poll(&server.path(), Some(request), vec![], 1)
            .is_err()
    );
    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    assert!(worker.take_ready(&snapshot, Instant::now()).is_none());
    assert!(!state.outputs[0].show_notifications);
    assert_eq!(server.task.join().unwrap().len(), 2);
}

#[test]
fn dismissal_after_acknowledgement_cancels_recovery_before_open_is_queued() {
    let server = Server::new(vec![Reply::Ack, Reply::Timeout, Reply::Snapshot]);
    let (mut state, request) = request(&server.path());
    let mut worker = Worker::default();
    assert!(
        worker
            .poll(&server.path(), Some(request), vec![], 1)
            .is_err()
    );
    state.dismiss_notifications("eDP-1");
    server.release.send(()).unwrap();
    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    assert!(worker.take_ready(&snapshot, Instant::now()).is_none());
    assert!(!state.outputs[0].show_notifications);
    assert_eq!(server.task.join().unwrap().len(), 3);
}

#[test]
fn repeated_query_timeouts_do_not_renew_the_total_intent_deadline() {
    let server = Server::new(vec![
        Reply::Ack,
        Reply::Timeout,
        Reply::Timeout,
        Reply::Snapshot,
    ]);
    let (state, request) = request(&server.path());
    let mut worker = Worker::default();
    assert!(
        worker
            .poll(&server.path(), Some(request), vec![], 1)
            .is_err()
    );
    let first_failure = Instant::now();
    server.release.send(()).unwrap();
    assert!(worker.poll(&server.path(), None, vec![], 2).is_err());
    server.release.send(()).unwrap();
    let snapshot = worker.poll(&server.path(), None, vec![], 3).unwrap();
    // Past the original acknowledgement deadline, but before a deadline renewed
    // at the second failure. No five-second sleep is needed to cover this bound.
    assert!(
        worker
            .take_ready(&snapshot, first_failure + Duration::from_secs(5))
            .is_none()
    );
    assert!(!state.outputs[0].show_notifications);
    let commands = server.task.join().unwrap();
    assert_eq!(commands.len(), 4);
    assert!(commands[0].starts_with("frame-popup-anchor "));
    assert!(
        commands[1..]
            .iter()
            .all(|command| command == "layout-status\n")
    );
}

#[test]
fn recovery_rejects_a_missed_takeover_even_after_availability_returns() {
    let server = Server::new(vec![Reply::Ack, Reply::Timeout, Reply::Epoch(1)]);
    let (state, request) = request(&server.path());
    let mut worker = Worker::default();
    assert!(
        worker
            .poll(&server.path(), Some(request), vec![], 1)
            .is_err()
    );
    server.release.send(()).unwrap();
    let snapshot = worker.poll(&server.path(), None, vec![], 2).unwrap();
    assert!(!snapshot.editing && snapshot.confirmation.is_none());
    assert!(!snapshot.outputs.is_empty());
    assert!(worker.take_ready(&snapshot, Instant::now()).is_none());
    assert!(!state.outputs[0].show_notifications);
    assert_eq!(server.task.join().unwrap().len(), 3);
}

#[test]
fn observed_epoch_cancels_queued_open_before_the_ui_applies_the_new_snapshot() {
    let server = Server::new(vec![Reply::Ack, Reply::Snapshot, Reply::Epoch(1)]);
    let (mut state, request) = request(&server.path());
    let mut worker = Worker::default();
    let snapshot = worker
        .poll(&server.path(), Some(request), vec![], 1)
        .unwrap();
    let queued = worker.take_ready(&snapshot, Instant::now()).unwrap();
    let recovered = worker.poll(&server.path(), None, vec![], 2).unwrap();
    assert_ne!(
        recovered.popup_epoch,
        state.frame_layout.snapshot.popup_epoch
    );
    // Deliver the older Open before the queued Snapshot, as the real UI would.
    queued.deliver(&mut state, Instant::now());
    assert!(!state.outputs[0].show_notifications);
    assert!(worker.take_ready(&recovered, Instant::now()).is_none());
    assert_eq!(server.task.join().unwrap().len(), 3);
}

#[test]
fn permanent_observation_failures_revoke_an_already_dispatched_open() {
    for failure in [
        Reply::Truncated,
        Reply::InvalidHeader,
        Reply::MalformedSnapshot,
    ] {
        let server = Server::new(vec![Reply::Ack, Reply::Snapshot, failure]);
        let (mut state, request) = request(&server.path());
        let mut worker = Worker::default();
        let snapshot = worker
            .poll(&server.path(), Some(request), vec![], 1)
            .unwrap();
        let queued = worker.take_ready(&snapshot, Instant::now()).unwrap();
        assert!(worker.poll(&server.path(), None, vec![], 2).is_err());
        queued.deliver(&mut state, Instant::now());
        assert!(!state.outputs[0].show_notifications);
        assert_eq!(server.task.join().unwrap().len(), 3);
    }
}

#[test]
fn connection_failure_revokes_dispatched_open_but_transient_timeout_does_not() {
    let server = Server::new(vec![Reply::Ack, Reply::Snapshot, Reply::Timeout]);
    let (mut state, request) = request(&server.path());
    let mut worker = Worker::default();
    let snapshot = worker
        .poll(&server.path(), Some(request), vec![], 1)
        .unwrap();
    let queued = worker.take_ready(&snapshot, Instant::now()).unwrap();
    assert!(worker.poll(&server.path(), None, vec![], 2).is_err());
    assert!(queued.valid_at(Instant::now()));
    server.release.send(()).unwrap();
    let path = server.path();
    assert_eq!(server.task.join().unwrap().len(), 3);
    assert!(worker.poll(&path, None, vec![], 3).is_err());
    queued.deliver(&mut state, Instant::now());
    assert!(!state.outputs[0].show_notifications);
}

#[test]
fn fresh_intent_after_cancellation_event_uses_new_epoch_before_snapshot_recovery() {
    let server = Server::new(vec![Reply::Ack, Reply::Timeout, Reply::Epoch(1)]);
    let (mut state, mut requests) = setup(&server.path());
    super::super::cancel_popups(&mut state, 1);
    assert_eq!(state.frame_layout.snapshot.popup_epoch, 0);
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
    let opening = worker.take_ready(&snapshot, Instant::now()).unwrap();
    super::super::apply(&mut state, snapshot);
    opening.deliver(&mut state, Instant::now());
    assert!(state.outputs[0].show_notifications);
    assert_eq!(server.task.join().unwrap().len(), 3);
}
