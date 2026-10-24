use super::*;
use hearthspace_ipc::{
    settings_snapshot::{self as protocol, SettingsSnapshot, WatchRequest},
    transport::request_line,
};
use std::{
    fs,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Barrier},
    thread,
};

const CHILD_SOCKET: &str = "HEARTHSPACE_TEST_SETTINGS_WATCH_SOCKET";
const CHILD_CONFIG: &str = "HEARTHSPACE_TEST_SETTINGS_WATCH_CONFIG";

#[test]
fn daemon_process() {
    let Some(socket) = std::env::var_os(CHILD_SOCKET) else {
        return;
    };
    let config = PathBuf::from(std::env::var_os(CHILD_CONFIG).unwrap());
    let listener = match super::super::bind_settings_socket_at(PathBuf::from(socket)).unwrap() {
        super::super::BindSettingsSocket::Listening(listener) => listener,
        _ => panic!("test daemon socket is already owned"),
    };
    serve(listener, &config).unwrap();
}

struct Daemon {
    child: Child,
    socket: PathBuf,
}
impl Daemon {
    fn start(socket: PathBuf, config: &Path) -> Self {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "server::watch_tests::daemon_process",
                "--nocapture",
            ])
            .env(CHILD_SOCKET, &socket)
            .env(CHILD_CONFIG, config)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut daemon = Self { child, socket };
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if protocol::client_get_snapshot_at(&daemon.socket).is_ok() {
                return daemon;
            }
            assert!(
                daemon.child.try_wait().unwrap().is_none(),
                "daemon exited during startup"
            );
            assert!(Instant::now() < deadline, "daemon did not become ready");
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn snapshot(&self) -> SettingsSnapshot {
        protocol::client_get_snapshot_at(&self.socket).unwrap()
    }
    fn set(&self, request: &str) -> String {
        request_line(&self.socket, request, Duration::from_secs(2)).unwrap()
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn competing_processes_preserve_appearance_display_and_notify_other_daemon() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("new/profile/settings.toml");
    let first = Daemon::start(directory.path().join("first.sock"), &config);
    let second = Daemon::start(directory.path().join("second.sock"), &config);
    let before = second.snapshot();
    let cursor = before.cursor();
    let watching = second.socket.clone();
    let wait = thread::spawn(move || {
        protocol::client_wait_for_change_at(&watching, &cursor, Duration::from_secs(2))
            .unwrap()
            .unwrap()
    });
    let barrier = Arc::new(Barrier::new(3));
    let mut layout = hearthspace_ipc::Settings::default().monitors;
    layout[0].scale = 2;
    layout[1].x = 960;
    let request = hearthspace_ipc::display_layout_request(&layout).unwrap();
    let workers: Vec<_> = [(&first, request), (&second, "set color-mode dark\n".into())]
        .into_iter()
        .map(|(daemon, request)| {
            let socket = daemon.socket.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                request_line(&socket, &request, Duration::from_secs(2)).unwrap()
            })
        })
        .collect();
    barrier.wait();
    for worker in workers {
        assert!(worker.join().unwrap().starts_with("ok "));
    }
    assert_ne!(wait.join().unwrap().cursor(), before.cursor());
    let final_snapshot = first.snapshot();
    assert_eq!(final_snapshot.revision, 2);
    assert_eq!(final_snapshot.settings.monitors, layout);
    assert_eq!(
        final_snapshot.settings.appearance.color_mode,
        hearthspace_ipc::ColorMode::Dark
    );
    assert_eq!(second.snapshot(), final_snapshot);
}

#[test]
fn invalid_noop_and_locked_writes_do_not_notify_and_lock_timeout_recovers() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("settings.toml");
    let daemon = Daemon::start(directory.path().join("settings.sock"), &config);
    assert!(daemon.set("set test true\n").starts_with("ok "));
    let before = daemon.snapshot();
    assert!(daemon.set("set test true\n").starts_with("ok "));
    assert!(
        daemon
            .set("set monitor-scale primary 0\n")
            .starts_with("err ")
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(config.with_extension("toml.lock"))
        .unwrap();
    lock.lock().unwrap();
    let started = Instant::now();
    assert!(daemon.set("set test false\n").contains("lock timed out"));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(daemon.snapshot(), before);
    assert!(
        protocol::client_wait_for_change_at(
            &daemon.socket,
            &before.cursor(),
            Duration::from_millis(40)
        )
        .unwrap()
        .is_none()
    );
    drop(lock);
    assert!(daemon.set("set test false\n").starts_with("ok "));
    assert!(daemon.snapshot().revision > before.revision);
}

#[test]
fn quiet_watchers_and_unfinished_headers_leave_room_for_setters() {
    let directory = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(
        directory.path().join("settings.sock"),
        &directory.path().join("settings.toml"),
    );
    let before = daemon.snapshot();
    let payload = serde_json::to_string(&WatchRequest {
        version: 1,
        after: before.cursor(),
        wait_ms: 5000,
    })
    .unwrap();
    let mut watchers = Vec::new();
    for _ in 0..MAX_WATCHERS {
        let mut stream = UnixStream::connect(&daemon.socket).unwrap();
        stream
            .write_all(format!("watch settings {payload}\n").as_bytes())
            .unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        watchers.push(stream);
    }
    // A following GET is a processing barrier for the earlier accepted watches.
    assert_eq!(daemon.snapshot(), before);
    let error = protocol::client_wait_for_change_at(
        &daemon.socket,
        &before.cursor(),
        Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(error.to_string().contains("capacity"));
    let mut incomplete = Vec::new();
    for _ in 0..MAX_UNFINISHED {
        let mut stream = UnixStream::connect(&daemon.socket).unwrap();
        stream.write_all(b"set ").unwrap();
        incomplete.push(stream);
    }
    let started = Instant::now();
    assert!(daemon.set("set test true\n").starts_with("ok "));
    assert!(started.elapsed() < Duration::from_millis(500));
    for mut watcher in watchers {
        watcher
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut response = String::new();
        watcher.read_to_string(&mut response).unwrap();
        assert!(response.contains("\"revision\":1"));
    }
}

#[test]
fn atomic_hand_edits_and_reconnect_observe_latest_without_rewriting_legacy() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("settings.toml");
    let socket = directory.path().join("settings.sock");
    let daemon = Daemon::start(socket.clone(), &config);
    let before = daemon.snapshot();
    let watching = socket.clone();
    let cursor = before.cursor();
    let wait = thread::spawn(move || {
        protocol::client_wait_for_change_at(&watching, &cursor, Duration::from_secs(2))
            .unwrap()
            .unwrap()
    });
    let temporary = directory.path().join("edit.tmp");
    let legacy = "color_mode = dark\nbackground_image_path = /tmp/photo #1.png\n";
    fs::write(&temporary, legacy).unwrap();
    fs::rename(temporary, &config).unwrap();
    let observed = wait.join().unwrap();
    assert_eq!(observed.revision, 0);
    assert_eq!(
        observed.settings.appearance.color_mode,
        hearthspace_ipc::ColorMode::Dark
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), legacy);
    drop(daemon);
    let daemon = Daemon::start(socket, &config);
    assert_eq!(daemon.snapshot(), observed);
    assert!(daemon.set("set test true\n").starts_with("ok "));
    assert_eq!(daemon.snapshot().revision, 1);
    assert!(
        protocol::client_wait_for_change_at(&daemon.socket, &observed.cursor(), Duration::ZERO)
            .unwrap()
            .is_some()
    );
}

#[test]
fn accumulated_percent_encoded_layouts_are_rejected_before_commit_or_revision_change() {
    use hearthspace_ipc::{
        MonitorConfig, display_layout_request,
        transport::{MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES},
    };
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("settings.toml");
    let daemon = Daemon::start(directory.path().join("settings.sock"), &config);
    let request = |index: i32| {
        let monitor = MonitorConfig {
            id: format!("{index}{}", "#".repeat(2500)),
            name: "#".repeat(2500),
            width: 1,
            height: 1,
            scale: 1,
            x: index * 10,
            y: 0,
        };
        let line = display_layout_request(&[monitor]).unwrap();
        assert!(line.len() < MAX_REQUEST_BYTES);
        line
    };
    for index in 0..4 {
        let response = daemon.set(&request(index));
        assert!(
            response.starts_with("ok "),
            "accepted prefix should fit: {response}"
        );
        assert!(response.len() <= MAX_RESPONSE_BYTES);
    }
    let before = daemon.snapshot();
    assert_eq!(before.revision, 4);
    let file = fs::read(&config).unwrap();
    let lock = fs::read(config.with_extension("toml.lock")).unwrap();
    let response = daemon.set(&request(4));
    assert!(response.starts_with("err "));
    assert!(response.contains("legacy settings response exceeds response limit"));
    assert_eq!(fs::read(&config).unwrap(), file);
    assert_eq!(fs::read(config.with_extension("toml.lock")).unwrap(), lock);
    assert_eq!(daemon.snapshot(), before);
    assert!(daemon.set("get\n").starts_with("ok "));
    assert!(
        protocol::client_wait_for_change_at(
            &daemon.socket,
            &before.cursor(),
            Duration::from_millis(40)
        )
        .unwrap()
        .is_none()
    );
}
