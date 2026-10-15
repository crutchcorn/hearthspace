use super::*;
use std::os::unix::net::UnixListener;

#[test]
fn stalled_peer_has_an_absolute_read_deadline() {
    let (mut reader, _peer) = UnixStream::pair().unwrap();
    let started = Instant::now();
    let error = read_line_until(&mut reader, 128, started + Duration::from_millis(30)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn trickling_bytes_cannot_extend_the_deadline() {
    let (mut reader, mut peer) = UnixStream::pair().unwrap();
    let sender = thread::spawn(move || {
        for _ in 0..12 {
            if peer.write_all(b"x").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    let started = Instant::now();
    let error = read_line_until(&mut reader, 128, started + Duration::from_millis(35)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(reader);
    sender.join().unwrap();
}

#[test]
fn response_is_complete_at_newline_without_waiting_for_disconnect() {
    let (mut reader, mut peer) = UnixStream::pair().unwrap();
    peer.write_all(b"ok\n").unwrap();
    assert_eq!(
        read_line_until(&mut reader, 128, Instant::now() + Duration::from_millis(30)).unwrap(),
        "ok\n"
    );
}

#[test]
fn oversized_and_truncated_responses_fail() {
    let (mut reader, mut peer) = UnixStream::pair().unwrap();
    peer.write_all(b"too large\n").unwrap();
    assert_eq!(
        read_line_until(&mut reader, 4, Instant::now() + Duration::from_secs(1))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let (mut reader, mut peer) = UnixStream::pair().unwrap();
    peer.write_all(b"ok").unwrap();
    drop(peer);
    assert_eq!(
        read_line_until(&mut reader, 4, Instant::now() + Duration::from_secs(1))
            .unwrap_err()
            .kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn full_listen_backlog_cannot_hang_connect() {
    use nix::sys::socket::{Backlog, listen};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("full.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listen(&listener, Backlog::new(0).unwrap()).unwrap();
    let _queued = UnixStream::connect(&path).unwrap();
    let started = Instant::now();
    assert_eq!(
        connect_until(&path, started + Duration::from_millis(30))
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn invalid_requests_are_rejected_before_connecting() {
    for request in [
        String::new(),
        "get".into(),
        "get\nget\n".into(),
        "x".repeat(MAX_REQUEST_BYTES + 1),
    ] {
        assert_eq!(
            request_line(Path::new("/missing"), &request, Duration::from_millis(30))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[test]
fn a_nonreading_peer_cannot_hang_a_write() {
    let (mut writer, _peer) = UnixStream::pair().unwrap();
    let started = Instant::now();
    assert_eq!(
        write_until(
            &mut writer,
            &vec![0; 2 * 1024 * 1024],
            started + Duration::from_millis(30)
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(started.elapsed() < Duration::from_secs(1));
}
