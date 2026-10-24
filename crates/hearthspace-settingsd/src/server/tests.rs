use super::*;
use hearthspace_ipc::transport::MAX_REQUEST_BYTES;
use std::{
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
};

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
        let mut client = Client::new(stream).unwrap();
        assert!(!client.poll(&config).unwrap());
        assert!(!config.exists());
        assert!(client.request.len() <= MAX_REQUEST_BYTES);
    }
}

#[test]
fn expired_peers_are_removed_without_reading_or_writing() {
    let dir = tempfile::tempdir().unwrap();
    let (stream, _peer) = UnixStream::pair().unwrap();
    let mut client = Client::new(stream).unwrap();
    client.deadline = Instant::now() - Duration::from_millis(1);
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

#[test]
fn concurrent_display_and_background_requests_keep_both_domains_and_whole_layouts() {
    use hearthspace_ipc::{Settings, display_layout_request};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.sock");
    let config = dir.path().join("config.toml");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut first_layout = Settings::default().monitors;
    first_layout[0].scale = 2;
    first_layout[1].x = 960;
    let mut last_layout = first_layout.clone();
    last_layout[1].x = 960;
    last_layout[1].y = 200;
    let requests = [
        display_layout_request(&first_layout).unwrap(),
        "set background-mode cover\n".into(),
        display_layout_request(&last_layout).unwrap(),
    ];
    let mut clients: Vec<_> = requests
        .iter()
        .map(|request| {
            let mut client = UnixStream::connect(&path).unwrap();
            client.write_all(request.as_bytes()).unwrap();
            client
                .set_read_timeout(Some(Duration::from_millis(500)))
                .unwrap();
            client
        })
        .collect();

    Server::default().poll(&listener, &config).unwrap();
    let mut first_response = String::new();
    clients[0].read_to_string(&mut first_response).unwrap();
    assert!(first_response.contains("monitor primary 1920 1080 2 0 0 Primary"));
    assert!(first_response.contains("monitor secondary 1920 1080 1 960 0 Secondary"));
    for client in &mut clients[1..] {
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("ok "), "{response}");
    }
    let saved = super::super::load_settings_from_path(config).unwrap();
    assert_eq!(saved.monitors, last_layout);
    assert_eq!(
        saved.background_mode,
        hearthspace_ipc::BackgroundMode::Cover
    );
}
