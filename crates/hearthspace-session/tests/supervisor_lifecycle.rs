use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "hearthspace-supervisor-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        script(
            &root.join("hearthspace-settingsd"),
            r#"#!/usr/bin/python3
import os, socket
with socket.socket(socket.AF_UNIX) as server:
    server.bind(os.environ['HEARTHSPACE_SETTINGS_SOCKET'])
    server.listen()
    while True:
        conn, _ = server.accept()
        with conn:
            conn.recv(1024)
            conn.sendall(b'ok {}\n')
"#,
        );
        script(
            &root.join("hearthspace-compositor"),
            "#!/bin/sh\nwhile true; do echo still-building; sleep 0.01; done\n",
        );
        Self(root)
    }

    fn start(&self, timeout: &str) -> Child {
        Command::new(env!("CARGO_BIN_EXE_hearthspace-session"))
            .args([
                "--installed",
                "--backend",
                "nested",
                "--startup-timeout",
                timeout,
                "--no-watch-apps",
            ])
            .arg("--binary-dir")
            .arg(&self.0)
            .arg("--session-dir")
            .arg(self.0.join("runtime"))
            .env("XDG_RUNTIME_DIR", &self.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn bounded_exit(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("supervisor did not exit before deadline");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn noisy_child_cannot_extend_absolute_startup_deadline() {
    let fixture = Fixture::new();
    let started = Instant::now();
    let mut child = fixture.start("1");
    assert!(!bounded_exit(&mut child).success());
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(!fixture.0.join("runtime").exists());
}

#[test]
fn sigterm_interrupts_startup_and_cleans_children() {
    let fixture = Fixture::new();
    let mut child = fixture.start("60");
    let deadline = Instant::now() + Duration::from_secs(3);
    let processes = fixture.0.join("runtime/processes.json");
    while !processes.exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    signal::kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).unwrap();
    assert!(!bounded_exit(&mut child).success());
    assert!(!fixture.0.join("runtime").exists());
}
