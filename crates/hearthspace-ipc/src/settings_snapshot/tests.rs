use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    thread,
    time::Instant,
};

#[test]
fn snapshot_round_trip_preserves_empty_layout_and_arbitrary_strings() {
    let mut settings = Settings {
        monitors: vec![],
        background_image_path: Some("/tmp/虹 # \"\n.png".into()),
        ..Settings::default()
    };
    settings.appearance.color_mode = crate::ColorMode::Dark;
    let snapshot = SettingsSnapshot::new(71, settings).unwrap();
    let line = snapshot_response(Some(snapshot.clone())).unwrap();
    assert_eq!(line.lines().count(), 1);
    assert_eq!(parse_response(&line).unwrap(), Some(snapshot));
}

#[test]
fn invalid_versions_cursors_and_fingerprints_are_rejected() {
    let snapshot = SettingsSnapshot::new(1, Settings::default()).unwrap();
    let response = snapshot_response(Some(snapshot)).unwrap();
    assert!(parse_response(&response.replace("\"version\":1", "\"version\":2")).is_err());
    assert!(parse_response(&response.replace("\"test\":false", "\"test\":true")).is_err());
    let request = WatchRequest {
        version: 1,
        after: SettingsSnapshot::new(0, Settings::default())
            .unwrap()
            .cursor(),
        wait_ms: 30_001,
    };
    assert!(WatchRequest::parse(&serde_json::to_string(&request).unwrap()).is_err());
    assert!(
        client_wait_for_change_at(
            Path::new("/missing"),
            &request.after,
            Duration::from_secs(31)
        )
        .is_err()
    );
}

#[test]
fn bounded_wait_and_reconnect_use_one_complete_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("settings.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let initial = SettingsSnapshot::new(3, Settings::default()).unwrap();
    let changed = SettingsSnapshot::new(
        4,
        Settings {
            test: true,
            ..Settings::default()
        },
    )
    .unwrap();
    let expected = changed.clone();
    let serve = thread::spawn(move || {
        for snapshot in [Some(initial), None, Some(changed)] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).unwrap();
            if request.starts_with("watch ") {
                let watch =
                    WatchRequest::parse(request.trim().strip_prefix("watch settings ").unwrap())
                        .unwrap();
                assert_eq!(watch.wait_ms, 10);
            } else {
                assert_eq!(request, "get snapshot\n");
            }
            stream
                .write_all(snapshot_response(snapshot).unwrap().as_bytes())
                .unwrap();
        }
    });
    let initial = client_get_snapshot_at(&socket).unwrap();
    assert!(
        client_wait_for_change_at(&socket, &initial.cursor(), Duration::from_millis(10))
            .unwrap()
            .is_none()
    );
    assert_eq!(client_get_snapshot_at(&socket).unwrap(), expected);
    serve.join().unwrap();
}

#[test]
fn a_stalled_watch_peer_cannot_extend_its_absolute_budget() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("settings.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let serve = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        for _ in 0..8 {
            if stream.write_all(b" ").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
    let started = Instant::now();
    let cursor = SettingsSnapshot::new(0, Settings::default())
        .unwrap()
        .cursor();
    assert!(client_wait_for_change_at(&socket, &cursor, Duration::from_millis(10)).is_err());
    assert!(started.elapsed() < Duration::from_millis(750));
    serve.join().unwrap();
}
