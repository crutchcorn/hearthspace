use super::*;
use crate::model::State;
use std::{
    net::Shutdown,
    sync::{Arc, Mutex},
};

#[test]
fn unfinished_peer_does_not_block_complete_half_closed_request() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut stalled = UnixStream::connect(&path).unwrap();
    stalled.write_all(b"{\"command\":").unwrap();
    let mut ready = UnixStream::connect(&path).unwrap();
    ready
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    ready.write_all(b"{\"command\":\"list\"}\n").unwrap();
    ready.shutdown(Shutdown::Write).unwrap();
    let mut server = Server::default();
    let state = Arc::new(Mutex::new(State::default()));
    server.poll(&listener, &state).unwrap();
    let mut response = String::new();
    ready.read_to_string(&mut response).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap(),
        json!({"ok": true, "notifications": []})
    );
    assert_eq!(server.clients.len(), 1);
    server.clients[0].deadline = Instant::now();
    server.poll(&listener, &state).unwrap();
    assert!(server.clients.is_empty());
}

#[test]
fn oversized_and_unknown_requests_are_rejected() {
    let (mut client, stream) = UnixStream::pair().unwrap();
    stream.set_nonblocking(true).unwrap();
    let mut peer = Client {
        stream,
        deadline: Instant::now() + Duration::from_secs(1),
        request: Vec::new(),
        response: None,
        written: 0,
    };
    client.write_all(&vec![b'x'; MAX_REQUEST + 1]).unwrap();
    let state = Arc::new(Mutex::new(State::default()));
    assert!(peer.poll(&state).unwrap());
    assert!(!peer.poll(&state).unwrap());
    assert!(peer.request.is_empty());
    assert_eq!(
        handle(br#"{"command":"list","extra":true}"#, &state)["ok"],
        false
    );
    assert_eq!(
        handle(br#"{"command":"action","id":1,"key":"exec"}"#, &state)["ok"],
        false
    );
}

#[test]
fn peer_count_remains_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let _peers: Vec<_> = (0..MAX_CLIENTS + 2)
        .map(|_| UnixStream::connect(&path).unwrap())
        .collect();
    let mut server = Server::default();
    server
        .poll(&listener, &Arc::new(Mutex::new(State::default())))
        .unwrap();
    assert_eq!(server.clients.len(), MAX_CLIENTS);
}
