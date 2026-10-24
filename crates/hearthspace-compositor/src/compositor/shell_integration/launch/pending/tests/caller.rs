//! Actual command connection + preparation worker, before any child is created.
use super::*;
use crate::compositor::shell_integration::command_socket::CommandSource;
use hearthspace_ipc::transport::launch::Envelope;
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    rc::Rc,
};

struct Fixture {
    events: EventLoop<'static, usize>,
    client: UnixStream,
    ready: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
    token: smithay::reexports::calloop::RegistrationToken,
    root: tempfile::TempDir,
    configured: usize,
}

impl Fixture {
    fn new(deadline: Option<Instant>, budget: Duration) -> Self {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("spawned");
        let mut events = EventLoop::try_new().unwrap();
        let handle = events.handle();
        let (server, mut client) = UnixStream::pair().unwrap();
        server.set_nonblocking(true).unwrap();
        let (entered, ready) = mpsc::channel();
        let (release, stalled) = mpsc::channel();
        let mut stalled = Some(stalled);
        let token = events
            .handle()
            .insert_source(
                CommandSource::new(server, Rc::default()).unwrap(),
                move |line, _, _| {
                    assert_eq!(line, b"launch-app fixture.desktop\n");
                    let stalled = stalled.take().expect("no retry");
                    let marker = marker.clone();
                    let entered = entered.clone();
                    start(
                        &handle,
                        budget,
                        move || {
                            entered.send(()).unwrap();
                            stalled.recv_timeout(Duration::from_secs(2)).unwrap();
                            let mut command = Command::new("/usr/bin/touch");
                            command.arg(marker);
                            Ok(command)
                        },
                        |command, _, configured| {
                            *configured += 1;
                            Ok(Execution::detached(command))
                        },
                    )
                },
            )
            .unwrap();
        let line = deadline.map_or_else(
            || "launch-app fixture.desktop\n".into(),
            |deadline| Envelope::encode("launch-app fixture.desktop", deadline).unwrap(),
        );
        client.write_all(line.as_bytes()).unwrap();
        client.set_nonblocking(true).unwrap();
        events.dispatch(Some(Duration::ZERO), &mut 0).unwrap();
        Self {
            events,
            client,
            ready,
            release,
            token,
            root,
            configured: 0,
        }
    }

    fn preparing(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if self.ready.try_recv().is_ok() {
                return;
            }
            assert!(Instant::now() < deadline, "preparation did not start");
            self.events
                .dispatch(Some(Duration::from_millis(1)), &mut self.configured)
                .unwrap();
        }
    }

    fn no_child_after_release(&mut self) {
        self.release.send(()).unwrap();
        // Deliberately do not dispatch the connection again before the worker
        // finishes: the final guard itself must observe caller revocation.
        workers_finished();
        self.events
            .dispatch(Some(Duration::ZERO), &mut self.configured)
            .unwrap();
        assert_eq!(self.configured, 0);
        assert!(!self.root.path().join("spawned").exists());
    }

    fn accepted(&mut self) {
        self.release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut reply = Vec::new();
        while !reply.contains(&b'\n') && Instant::now() < deadline {
            self.events
                .dispatch(Some(Duration::from_millis(1)), &mut self.configured)
                .unwrap();
            let _ = self.client.read_to_end(&mut reply);
        }
        assert_eq!(reply, b"ok\n");
        workers_finished();
        while !self.root.path().join("spawned").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(self.root.path().join("spawned").exists());
    }
}

#[test]
fn opted_in_disconnect_during_preparation_cannot_create_a_child() {
    let _test = TESTS.lock().unwrap();
    let mut fixture = Fixture::new(
        Some(Instant::now() + Duration::from_secs(1)),
        Duration::from_secs(1),
    );
    fixture.preparing();
    fixture.client.shutdown(Shutdown::Both).unwrap();
    fixture.no_child_after_release();
}

#[test]
fn opted_in_cancellation_half_close_during_preparation_cannot_create_a_child() {
    let _test = TESTS.lock().unwrap();
    let mut fixture = Fixture::new(
        Some(Instant::now() + Duration::from_secs(1)),
        Duration::from_secs(1),
    );
    fixture.preparing();
    fixture.client.shutdown(Shutdown::Write).unwrap();
    fixture.no_child_after_release();
}

#[test]
fn removing_the_connection_during_preparation_revokes_the_worker() {
    let _test = TESTS.lock().unwrap();
    let mut fixture = Fixture::new(
        Some(Instant::now() + Duration::from_secs(1)),
        Duration::from_secs(1),
    );
    fixture.preparing();
    fixture.events.handle().remove(fixture.token);
    fixture.no_child_after_release();
}

#[test]
fn queued_caller_deadline_expires_during_preparation_even_with_a_live_peer() {
    let _test = TESTS.lock().unwrap();
    // Scale the review's queued 7s + preparation 4s case: the original caller
    // budget is partly consumed before transport, while the worker gets 1s.
    let deadline = Instant::now() + Duration::from_millis(500);
    std::thread::sleep(Duration::from_millis(100));
    let mut fixture = Fixture::new(Some(deadline), Duration::from_secs(1));
    fixture.preparing();
    std::thread::sleep(
        deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(5),
    );
    fixture.no_child_after_release();
}

#[test]
fn caller_cannot_extend_the_existing_preparation_budget() {
    let _test = TESTS.lock().unwrap();
    let mut fixture = Fixture::new(
        Some(Instant::now() + Duration::from_secs(10)),
        Duration::from_millis(40),
    );
    fixture.preparing();
    std::thread::sleep(Duration::from_millis(50));
    fixture.no_child_after_release();
}

#[test]
fn legacy_half_close_still_accepts_a_prepared_launch() {
    let _test = TESTS.lock().unwrap();
    let mut fixture = Fixture::new(None, Duration::from_secs(1));
    fixture.client.shutdown(Shutdown::Write).unwrap();
    fixture.preparing();
    fixture.accepted();
}

#[test]
fn dropping_an_unattached_internal_reply_keeps_the_launch_alive() {
    let _test = TESTS.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("internal-spawned");
    let destination = marker.clone();
    let mut events = EventLoop::<()>::try_new().unwrap();
    let (release, wait) = mpsc::channel();
    drop(start(
        &events.handle(),
        Duration::from_secs(1),
        move || {
            wait.recv_timeout(Duration::from_secs(2)).unwrap();
            let mut command = Command::new("/usr/bin/touch");
            command.arg(destination);
            Ok(command)
        },
        |command, _, _| Ok(Execution::detached(command)),
    ));
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !marker.exists() && Instant::now() < deadline {
        events
            .dispatch(Some(Duration::from_millis(1)), &mut ())
            .unwrap();
    }
    assert!(marker.exists());
    workers_finished();
}
