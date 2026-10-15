use super::*;
use std::{
    fs,
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn status(native: bool) -> Value {
    serde_json::json!({
        "backend": if native { "native" } else { "headless" },
        "active": true, "shell_ready": true, "locked": false, "recovery_ready": true,
        "outputs": [{"name": "eDP-1", "frame_id": 8}, {"name": "DP-2", "frame_id": 12}],
    })
}

fn capture(output: &str, frame: u64, presentation: &str) -> Vec<u8> {
    let mut value = serde_json::to_vec(&serde_json::json!({
        "output": output, "frame_id": frame, "presentation": presentation,
    }))
    .unwrap();
    value.extend_from_slice(b"\n\x89PNG\r\n\x1a\nfixture");
    value
}

fn listener() -> (PathBuf, UnixListener) {
    let path = std::env::temp_dir().join(format!(
        "hearthspace-desktop-ready-{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    let listener = UnixListener::bind(&path).unwrap();
    (path, listener)
}

#[test]
fn readiness_requires_shell_input_active_session_and_outputs() {
    let baseline = status(true);
    assert!(ready_outputs(&baseline, true).is_some());
    for (field, value) in [
        ("shell_ready", Value::Bool(false)),
        ("recovery_ready", Value::Bool(false)),
        ("active", Value::Bool(false)),
        ("locked", Value::Bool(true)),
        ("outputs", serde_json::json!([])),
        ("backend", Value::String("nested".into())),
    ] {
        let mut wrong = baseline.clone();
        wrong[field] = value;
        assert!(ready_outputs(&wrong, true).is_none(), "accepted {field}");
    }
    let mut missing = baseline.clone();
    missing.as_object_mut().unwrap().remove("shell_ready");
    assert!(ready_outputs(&missing, true).is_none());
    for name in ["", "DP-1\nquit", "DP 1"] {
        let mut wrong = baseline.clone();
        wrong["outputs"][0]["name"] = name.into();
        assert!(ready_outputs(&wrong, true).is_none());
    }
}

#[test]
fn native_requires_new_flip_not_old_or_offscreen_capture() {
    validate_capture(&capture("eDP-1", 9, "page-flip"), "eDP-1", 8, true).unwrap();
    for payload in [
        capture("eDP-1", 8, "page-flip"),
        capture("DP-2", 9, "page-flip"),
        capture("eDP-1", 9, "offscreen"),
        b"{}\nnot-a-png".to_vec(),
    ] {
        assert!(validate_capture(&payload, "eDP-1", 8, true).is_err());
    }
}

#[test]
fn nested_and_headless_keep_their_offscreen_contract() {
    let mut value = status(false);
    for backend in ["headless", "nested"] {
        value["backend"] = backend.into();
        assert!(ready_outputs(&value, false).is_some());
        validate_capture(&capture("eDP-1", 9, "offscreen"), "eDP-1", 8, false).unwrap();
    }
}

#[test]
fn every_output_requires_a_capture_and_hotplug_invalidates_the_attempt() {
    for hotplug in [false, true] {
        let (path, listener) = listener();
        let worker = thread::spawn(move || {
            for (index, expected) in ["status\n", "capture eDP-1\n", "capture DP-2\n", "status\n"]
                .into_iter()
                .enumerate()
            {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = vec![0; expected.len()];
                socket.read_exact(&mut request).unwrap();
                assert_eq!(request, expected.as_bytes());
                let payload = match index {
                    1 => capture("eDP-1", 9, "page-flip"),
                    2 => capture("DP-2", 13, "page-flip"),
                    _ => {
                        let mut value = status(true);
                        if index == 3 && hotplug {
                            value["outputs"].as_array_mut().unwrap().pop();
                        }
                        serde_json::to_vec(&value).unwrap()
                    }
                };
                socket
                    .write_all(format!("ok {}\n", payload.len()).as_bytes())
                    .unwrap();
                socket.write_all(&payload).unwrap();
            }
        });
        assert_eq!(
            probe(
                &path,
                true,
                Instant::now() + Duration::from_secs(2),
                &AtomicBool::new(false)
            )
            .unwrap(),
            !hotplug
        );
        worker.join().unwrap();
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn trickling_header_and_partial_payload_share_one_absolute_deadline() {
    for prefix in [b"o".as_slice(), b"ok 100\n{".as_slice()] {
        let (path, listener) = listener();
        let worker = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut command = [0; 7];
            socket.read_exact(&mut command).unwrap();
            socket.write_all(prefix).unwrap();
            for _ in 0..20 {
                thread::sleep(Duration::from_millis(15));
                if socket.write_all(b"x").is_err() {
                    break;
                }
            }
        });
        let started = Instant::now();
        assert!(
            request(
                &path,
                "status",
                STATUS_LIMIT,
                started + Duration::from_millis(60),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(started.elapsed() < Duration::from_millis(500));
        worker.join().unwrap();
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn cancellation_interrupts_a_stalled_payload_before_startup_timeout() {
    let (mut reader, _writer) = UnixStream::pair().unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        signal.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    let error = read_bounded(
        &mut reader,
        &mut [0],
        started + Duration::from_secs(5),
        &cancelled,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(started.elapsed() < Duration::from_millis(500));
    worker.join().unwrap();
}

#[test]
fn oversized_reply_is_rejected_without_allocating_its_claimed_payload() {
    let (path, listener) = listener();
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut command = [0; 7];
        socket.read_exact(&mut command).unwrap();
        socket
            .write_all(format!("ok {}\n", STATUS_LIMIT + 1).as_bytes())
            .unwrap();
    });
    assert!(
        request(
            &path,
            "status",
            STATUS_LIMIT,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false)
        )
        .is_err()
    );
    worker.join().unwrap();
    fs::remove_file(path).unwrap();
}
