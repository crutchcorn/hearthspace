use std::{net::TcpListener, os::unix::fs::PermissionsExt};

use clap::Parser;

use super::*;

fn supervisor(name: &str, body: &str) -> Supervisor {
    let directory =
        std::env::temp_dir().join(format!("hearthspace-wayvnc-{name}-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let script = directory.join("fake-wayvnc");
    fs::write(&script, format!("#!/usr/bin/python3\n{body}")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let args = Args::parse_from(["session", "--dev", "--vnc"]);
    let paths = SessionPaths {
        host_runtime_dir: directory.clone(),
        session_dir: directory.clone(),
        applications_dir: directory.join("share"),
        settings_socket: directory.join("settings.sock"),
        session_socket: directory.join("session.sock"),
        shell_command_socket: directory.join("command.sock"),
    };
    Supervisor::new(
        RunMode::DevCargo,
        paths,
        SessionEnv {
            wayland_display: "wayland-test".into(),
            vars: Vec::new(),
        },
        Arc::new(AtomicBool::new(false)),
        &args,
        Some(crate::wayvnc::Configuration {
            program: script,
            port,
        }),
    )
}

const SERVER: &str = r#"import os, socket, sys
control = socket.socket(socket.AF_UNIX)
control.bind(sys.argv[sys.argv.index('--socket') + 1])
server = socket.socket()
server.bind(('127.0.0.1', int(sys.argv[-1])))
server.listen()
while True:
    peer, _ = server.accept()
    with peer:
        peer.sendall(b'RFB 003.008\n')
"#;

#[test]
fn ready_child_is_recorded_and_reaped_with_the_session() {
    let mut supervisor = supervisor("lifecycle", SERVER);
    supervisor.start_wayvnc();
    let pid = supervisor.wayvnc.as_ref().expect("WayVNC readiness").pid();
    let processes: serde_json::Value = serde_json::from_slice(
        &fs::read(supervisor.paths.session_dir.join("processes.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(processes["wayvnc"], pid);
    let status: serde_json::Value = serde_json::from_slice(
        &fs::read(supervisor.paths.session_dir.join("wayvnc.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(status["ready"], true);
    supervisor.shutdown().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
    assert!(!supervisor.paths.session_dir.exists());
}

#[test]
fn optional_startup_failure_preserves_the_desktop_runtime() {
    let mut supervisor = supervisor("startup-failure", "raise SystemExit(7)\n");
    supervisor.start_wayvnc();
    assert!(supervisor.wayvnc.is_none());
    assert!(supervisor.paths.session_dir.exists());
    let status: serde_json::Value = serde_json::from_slice(
        &fs::read(supervisor.paths.session_dir.join("wayvnc.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(status["ready"], false);
    assert!(
        status["error"]
            .as_str()
            .unwrap()
            .contains("exited before readiness")
    );
    supervisor.shutdown().unwrap();
}

#[test]
fn child_exit_disables_sharing_without_ending_the_desktop() {
    let mut supervisor = supervisor("runtime-failure", SERVER);
    supervisor.start_wayvnc();
    let pid = supervisor.wayvnc.as_ref().expect("WayVNC readiness").pid();
    terminate_process(&mut supervisor.wayvnc);
    let deadline = Instant::now() + Duration::from_secs(2);
    while supervisor.wayvnc.is_some() {
        assert!(Instant::now() < deadline);
        supervisor.poll_wayvnc();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(supervisor.paths.session_dir.exists());
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
    let processes: serde_json::Value = serde_json::from_slice(
        &fs::read(supervisor.paths.session_dir.join("processes.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(processes["wayvnc"], 0);
    supervisor.shutdown().unwrap();
}
