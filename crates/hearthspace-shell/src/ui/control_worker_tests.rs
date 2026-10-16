//! Private compositor transport tests; no audio server or hardware is contacted.
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::{Arc, Mutex, atomic::AtomicBool},
    thread,
    time::Instant,
};

use serde_json::{Value, json};

use super::*;

struct Server {
    _directory: tempfile::TempDir,
    path: PathBuf,
    state: Arc<Mutex<ServerState>>,
    stopped: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

struct ServerState {
    status: Vec<u8>,
    status_delay: Duration,
    tokens: HashSet<u64>,
    requests: Vec<String>,
}

impl Server {
    fn new(status: Value) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let state = Arc::new(Mutex::new(ServerState {
            status: serde_json::to_vec(&status).unwrap(),
            status_delay: Duration::ZERO,
            tokens: HashSet::new(),
            requests: Vec::new(),
        }));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_stop = stopped.clone();
        let worker = thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                if worker_stop.load(Ordering::Relaxed) {
                    break;
                }
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut command = String::new();
                if BufReader::new(&stream).read_line(&mut command).is_err() {
                    continue;
                }
                let command = command.trim_end().to_string();
                let (response, delay) = {
                    let mut state = worker_state.lock().unwrap();
                    state.requests.push(command.clone());
                    if command == "status" {
                        let mut response = format!("ok {}\n", state.status.len()).into_bytes();
                        response.extend_from_slice(&state.status);
                        (response, state.status_delay)
                    } else {
                        let accepted = command
                            .strip_prefix("shell-event-accept ")
                            .and_then(|value| value.parse::<u64>().ok())
                            .is_some_and(|token| state.tokens.remove(&token));
                        let response = if accepted {
                            b"ok\n".to_vec()
                        } else {
                            b"err device control request expired, cancelled, or already accepted\n"
                                .to_vec()
                        };
                        (response, Duration::ZERO)
                    }
                };
                thread::sleep(delay);
                let _ = stream.write_all(&response);
            }
        });
        Self {
            _directory: directory,
            path,
            state,
            stopped,
            thread: Some(worker),
        }
    }

    fn status(&self, status: Value) {
        self.state.lock().unwrap().status = serde_json::to_vec(&status).unwrap();
    }

    fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = UnixStream::connect(&self.path);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn status(backend: &str) -> Value {
    json!({ "version": 1, "backend": backend, "active": true,
        "locked": false, "recovery_ready": true })
}

fn request(token: Option<u64>) -> Request {
    Request {
        id: 1,
        output: "test-output".into(),
        action: Action::SetBrightness(50),
        token,
        created: Instant::now(),
    }
}

#[tokio::test]
async fn session_requires_supported_version_backend_and_explicit_readiness_booleans() {
    let server = Server::new(status("native"));
    for backend in ["native", "nested", "headless"] {
        server.status(status(backend));
        let observed = session(server.path.clone()).await;
        assert!(observed.available);
        assert_eq!(observed.native, backend == "native");
    }
    let mut malformed = vec![Value::Null, json!([]), json!({})];
    for (field, invalid) in [
        ("version", json!(2)),
        ("version", json!("1")),
        ("backend", json!("unknown")),
        ("backend", json!(true)),
        ("active", json!("true")),
        ("locked", json!(0)),
        ("recovery_ready", json!(1)),
    ] {
        let mut value = status("nested");
        value[field] = invalid;
        malformed.push(value);
    }
    for field in ["version", "backend", "active", "locked", "recovery_ready"] {
        let mut value = status("nested");
        value.as_object_mut().unwrap().remove(field);
        malformed.push(value);
    }
    for value in malformed {
        server.status(value.clone());
        assert!(
            !session(server.path.clone()).await.available,
            "accepted {value}"
        );
    }
    server.state.lock().unwrap().status = b"{invalid json".to_vec();
    assert!(!session(server.path.clone()).await.available);
}

#[tokio::test]
async fn unavailable_socket_and_stalled_status_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let observed = session(directory.path().join("missing.sock")).await;
    assert!(!observed.available && !observed.native);
    let server = Server::new(status("nested"));
    server.state.lock().unwrap().status_delay = Duration::from_millis(850);
    assert!(!session(server.path.clone()).await.available);
}

#[tokio::test]
async fn expired_requests_are_rejected_before_any_transport_or_backend_discovery() {
    let server = Server::new(status("nested"));
    let mut request = request(Some(42));
    request.created = Instant::now() - Duration::from_secs(3);
    let (result, authorized) = adjust(&server.path, &request).await;
    assert!(result.unwrap_err().contains("expired"));
    assert!(!authorized);
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn inactive_locked_and_unrecovered_requests_stop_before_device_discovery() {
    let server = Server::new(status("nested"));
    for (field, value) in [
        ("active", false),
        ("locked", true),
        ("recovery_ready", false),
    ] {
        let mut unavailable = status("nested");
        unavailable[field] = json!(value);
        server.status(unavailable);
        let (result, authorized) = adjust(&server.path, &request(Some(42))).await;
        assert!(result.unwrap_err().contains("inactive or locked"));
        assert!(!authorized);
    }
    assert!(server.requests().iter().all(|command| command == "status"));
}

#[tokio::test]
async fn permission_is_not_reused_after_lock_or_vt_loss() {
    let server = Server::new(status("nested"));
    let request = request(None);
    accept(&server.path, &request).await.unwrap();
    for (field, value) in [("active", false), ("locked", true)] {
        let mut unavailable = status("nested");
        unavailable[field] = json!(value);
        server.status(unavailable);
        assert!(accept(&server.path, &request).await.is_err());
    }
}

#[tokio::test]
async fn media_tokens_are_single_use_and_invalid_tokens_cannot_authorize_feedback() {
    let server = Server::new(status("nested"));
    server.state.lock().unwrap().tokens.insert(42);
    accept(&server.path, &request(Some(42))).await.unwrap();
    assert!(accept(&server.path, &request(Some(42))).await.is_err());
    assert!(accept(&server.path, &request(Some(999))).await.is_err());

    // A nested brightness request is a guaranteed backend no-op. Its error OSD
    // still needs a valid media token; replay must not authorize that feedback.
    let (result, authorized) = adjust(&server.path, &request(Some(42))).await;
    assert!(result.unwrap_err().contains("native Hearthspace session"));
    assert!(!authorized);
    server.state.lock().unwrap().tokens.insert(43);
    let (result, authorized) = adjust(&server.path, &request(Some(43))).await;
    assert!(result.is_err());
    assert!(authorized);
    assert!(!server.state.lock().unwrap().tokens.contains(&43));
}

#[tokio::test]
async fn a_request_that_expires_while_status_is_in_flight_cannot_authorize_a_write() {
    let server = Server::new(status("nested"));
    server.state.lock().unwrap().status_delay = Duration::from_millis(350);
    let mut request = request(None);
    request.created = Instant::now() - Duration::from_millis(1750);
    assert!(accept(&server.path, &request).await.is_err());
    assert_eq!(server.requests(), ["status"]);
}
