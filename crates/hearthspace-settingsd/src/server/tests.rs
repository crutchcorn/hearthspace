use super::*;
use std::net::Shutdown;

#[test]
fn an_unfinished_client_does_not_delay_another_settings_request() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut stalled = UnixStream::connect(&path).unwrap();
    stalled.write_all(b"set test ").unwrap();
    let mut healthy = UnixStream::connect(&path).unwrap();
    healthy.write_all(b"get\n").unwrap();
    healthy
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut server = Server::default();
    server
        .poll(&listener, &dir.path().join("config.toml"))
        .unwrap();
    let mut response = String::new();
    healthy.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("ok test false "));
    assert_eq!(server.clients.len(), 1);
}

#[test]
fn oversized_or_truncated_requests_cannot_change_settings() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    for request in [b"set test true".to_vec(), vec![b'x'; MAX_REQUEST_BYTES + 1]] {
        let (stream, mut peer) = UnixStream::pair().unwrap();
        stream.set_nonblocking(true).unwrap();
        peer.write_all(&request).unwrap();
        peer.shutdown(Shutdown::Write).unwrap();
        let mut client = Client {
            stream,
            deadline: Instant::now() + CLIENT_TIMEOUT,
            request: Vec::new(),
            response: None,
            written: 0,
        };
        assert!(!client.poll(&config).unwrap());
        assert!(!config.exists());
        assert!(client.request.len() <= MAX_REQUEST_BYTES);
    }
}

#[test]
fn expired_peers_are_removed_without_reading_or_writing() {
    let dir = tempfile::tempdir().unwrap();
    let (stream, _peer) = UnixStream::pair().unwrap();
    let mut client = Client {
        stream,
        deadline: Instant::now() - Duration::from_millis(1),
        request: Vec::new(),
        response: None,
        written: 0,
    };
    assert!(!client.poll(&dir.path().join("config.toml")).unwrap());
}

#[test]
fn writes_are_serialized_without_losing_other_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.sock");
    let config = dir.path().join("config.toml");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut first = UnixStream::connect(&path).unwrap();
    first.write_all(b"set test true\n").unwrap();
    let mut second = UnixStream::connect(&path).unwrap();
    second.write_all(b"set background-mode cover\n").unwrap();
    Server::default().poll(&listener, &config).unwrap();
    let saved = super::super::load_settings_from_path(&config).unwrap();
    assert!(saved.test);
    assert_eq!(
        saved.background_mode,
        hearthspace_ipc::BackgroundMode::Cover
    );
}
