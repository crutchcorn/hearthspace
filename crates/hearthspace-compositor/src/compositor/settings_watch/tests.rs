use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
};

fn response(revision: u64) -> String {
    let settings = hearthspace_ipc::Settings {
        test: !revision.is_multiple_of(2),
        ..Default::default()
    };
    settings_snapshot::snapshot_response(Some(SettingsSnapshot::new(revision, settings).unwrap()))
        .unwrap()
}

struct Worker {
    subscription: Subscription,
    join: Option<thread::JoinHandle<()>>,
    _source: ping::PingSource,
}

impl Worker {
    fn new(socket: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        let (ping, source) = ping::make_ping().unwrap();
        let (send, receive) = mpsc::sync_channel(MAX_REFRESHES);
        let worker_stop = stop.clone();
        let worker_mailbox = mailbox.clone();
        let join = thread::spawn(move || run(socket, worker_stop, receive, worker_mailbox, ping));
        Self {
            subscription: Subscription {
                stop,
                mailbox,
                requests: send,
                worker: join.thread().clone(),
            },
            join: Some(join),
            _source: source,
        }
    }

    fn wait(&self, predicate: impl Fn(&Mailbox) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !predicate(&self.subscription.mailbox.lock().unwrap()) {
            assert!(
                std::time::Instant::now() < deadline,
                "settings worker did not publish"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.subscription.stop.store(true, Ordering::Release);
        self.subscription.worker.unpark();
        self.join.take().unwrap().join().unwrap();
    }
}

#[test]
fn subscription_retains_last_commit_on_disconnect_and_refreshes_on_reconnect() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("settings.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (released, proceed) = mpsc::channel();
    let server = thread::spawn(move || {
        for (prefix, reply) in [
            ("get snapshot", response(1)),
            ("watch settings ", "err offline\n".into()),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            assert!(line.starts_with(prefix));
            stream.write_all(reply.as_bytes()).unwrap();
        }
        proceed.recv_timeout(Duration::from_secs(3)).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(
            line, "get snapshot\n",
            "reconnect must replace its old cursor"
        );
        stream.write_all(response(2).as_bytes()).unwrap();
    });
    let worker = Worker::new(socket);
    worker.wait(|mailbox| mailbox.latest.as_ref().is_some_and(|s| s.revision == 1));
    assert_eq!(
        worker
            .subscription
            .mailbox
            .lock()
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .revision,
        1
    );
    released.send(()).unwrap();
    worker.wait(|mailbox| mailbox.latest.as_ref().is_some_and(|s| s.revision == 2));
    server.join().unwrap();
}

#[test]
fn explicit_refresh_is_bounded_and_acknowledgment_waits_for_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("settings.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (waiting, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        waiting.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(3)).unwrap();
        stream.write_all(response(3).as_bytes()).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        line.clear();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line, "get snapshot\n");
        stream.write_all(response(4).as_bytes()).unwrap();
    });
    let worker = Worker::new(socket);
    started.recv_timeout(Duration::from_secs(3)).unwrap();
    let mut receives = Vec::new();
    for _ in 0..MAX_REFRESHES {
        let ControlReply::Deferred(receive) = worker.subscription.refresh() else {
            panic!("refresh not deferred");
        };
        receives.push(receive);
    }
    assert!(matches!(
        worker.subscription.refresh(),
        ControlReply::Err(_)
    ));
    release.send(()).unwrap();
    worker.wait(|mailbox| mailbox.completed.len() == MAX_REFRESHES);
    assert!(
        receives
            .iter()
            .all(|reply| matches!(reply.try_recv(), Err(mpsc::TryRecvError::Empty)))
    );
    // The Calloop side owns application and acknowledgment, never the worker.
    let mut mailbox = worker.subscription.mailbox.lock().unwrap();
    assert_eq!(mailbox.latest.as_ref().unwrap().revision, 4);
    for (reply, result) in mailbox.completed.drain(..) {
        reply
            .send(result.map_or_else(ControlReply::Err, |_| ControlReply::Ok))
            .unwrap();
    }
    mailbox.pending = 0;
    drop(mailbox);
    assert!(
        receives
            .iter()
            .all(|reply| matches!(reply.recv().unwrap(), ControlReply::Ok))
    );
    server.join().unwrap();
}
