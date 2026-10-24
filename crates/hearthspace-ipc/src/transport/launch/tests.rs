use super::*;
use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixListener,
    sync::{Arc, mpsc},
};

#[test]
fn cancellation_while_waiting_revokes_the_open_request_socket() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (accepted, ready) = mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut peer = BufReader::new(stream);
        let mut line = String::new();
        peer.read_line(&mut line).unwrap();
        let envelope = Envelope::decode(line.as_bytes()).unwrap().unwrap();
        assert_eq!(envelope.command, "launch-app editor.desktop");
        accepted.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(
            peer.read(&mut byte).unwrap(),
            0,
            "cancellation must close the caller"
        );
    });
    let cancelled = Arc::new(Cancellation::default());
    let cancel = cancelled.clone();
    let caller = thread::spawn(move || {
        request(
            &path,
            "launch-app editor.desktop",
            Instant::now() + Duration::from_secs(1),
            &cancel,
        )
    });
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    cancelled.cancel();
    assert!(
        caller
            .join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    server.join().unwrap();
}

#[test]
fn queued_deadline_is_transferred_without_renewal_and_expired_envelopes_are_rejected() {
    let deadline = Instant::now() + Duration::from_millis(500);
    let line = Envelope::encode("launch-app editor.desktop", deadline).unwrap();
    thread::sleep(Duration::from_millis(20));
    let decoded = Envelope::decode(line.as_bytes()).unwrap().unwrap();
    assert!(decoded.deadline <= deadline);
    assert!(decoded.deadline.duration_since(Instant::now()) < Duration::from_millis(485));
    let expired = format!("{PREFIX}1 launch-app editor.desktop\n");
    assert_eq!(
        Envelope::decode(expired.as_bytes()).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(Envelope::decode(b"status\n").unwrap().is_none());
    assert!(Envelope::encode("capture", deadline).is_err());
}

#[test]
fn cancellation_before_registration_cannot_publish_a_request() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let (stream, _peer) = UnixStream::pair().unwrap();
    assert!(cancellation.register(&stream).is_err());
    assert!(
        request(
            &path,
            "launch-app editor.desktop",
            Instant::now() + Duration::from_secs(1),
            &cancellation,
        )
        .unwrap_err()
        .to_string()
        .contains("cancelled")
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn cancellation_revokes_registered_socket_without_worker_progress() {
    let cancellation = Cancellation::default();
    let (stream, mut peer) = UnixStream::pair().unwrap();
    let _registration = cancellation.register(&stream).unwrap();
    peer.set_nonblocking(true).unwrap();
    cancellation.cancel();
    // Keep both worker descriptors alive and do not run its read loop. The UI
    // cancellation itself must make EOF observable before preparation resumes.
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
}

#[test]
fn registration_drop_revokes_socket_before_the_original_descriptor_drops() {
    let cancellation = Cancellation::default();
    let (stream, mut peer) = UnixStream::pair().unwrap();
    let registration = cancellation.register(&stream).unwrap();
    peer.set_nonblocking(true).unwrap();
    drop(registration);
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    cancellation.cancel(); // Safe after the worker removed its registration.
}

#[test]
fn completed_request_does_not_retain_a_registered_socket() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut peer = BufReader::new(stream);
        let mut line = String::new();
        peer.read_line(&mut line).unwrap();
        assert_eq!(
            Envelope::decode(line.as_bytes()).unwrap().unwrap().command,
            "launch-app editor.desktop"
        );
        peer.get_mut().write_all(b"ok\n").unwrap();
        assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    });
    let cancellation = Cancellation::default();
    assert_eq!(
        request(
            &path,
            "launch-app editor.desktop",
            Instant::now() + Duration::from_secs(1),
            &cancellation,
        )
        .unwrap(),
        "ok\n"
    );
    // Leave cancellation alive and uncancelled while the peer observes EOF.
    server.join().unwrap();
    assert!(!cancellation.is_cancelled());
}
