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
            "#!/bin/sh\ntouch \"$HEARTHSPACE_HOST_XDG_RUNTIME_DIR/compositor-started\"\nwhile true; do echo still-building; sleep 0.01; done\n",
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
                "--no-desktop-services",
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
        if thread::panicking() {
            eprintln!("Keeping failed session fixture at {}", self.0.display());
            return;
        }
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

#[test]
fn private_dbus_session_can_initialize_and_cleanup_runtime() {
    let fixture = Fixture::new();
    let runtime = fixture.0.join("runtime");
    fs::create_dir(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let mut child = Command::new("dbus-run-session")
        .args([
            "--",
            env!("CARGO_BIN_EXE_hearthspace-session"),
            "--installed",
            "--backend",
            "nested",
            "--startup-timeout",
            "1",
            "--no-watch-apps",
            "--no-desktop-services",
            "--private-session-bus",
        ])
        .arg("--binary-dir")
        .arg(&fixture.0)
        .arg("--session-dir")
        .arg(&runtime)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", &fixture.0)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(!bounded_exit(&mut child).success());
    assert!(
        fixture.0.join("compositor-started").exists(),
        "supervisor rejected dbus-run-session's runtime bootstrap"
    );
    assert!(!runtime.exists());
}

#[test]
fn native_start_bootstraps_a_private_bus_and_correct_activation_environment() {
    let fixture = Fixture::new();
    let services = fixture.0.join("share/dbus-1/services");
    fs::create_dir_all(&services).unwrap();
    let probe = fixture.0.join("activation-probe");
    script(
        &probe,
        r#"#!/usr/bin/python3
import json, os
keys = ['XDG_RUNTIME_DIR', 'WAYLAND_DISPLAY', 'XDG_CURRENT_DESKTOP',
        'XDG_SESSION_TYPE', 'DISPLAY', 'WAYLAND_SOCKET', 'DBUS_SESSION_BUS_ADDRESS',
        'HEARTHSPACE_SETTINGS_SOCKET', 'PATH']
with open(os.environ['HEARTHSPACE_HOST_XDG_RUNTIME_DIR'] + '/activation.json', 'w') as output:
    json.dump({key: os.environ.get(key) for key in keys}, output)
"#,
    );
    fs::write(
        services.join("org.hearthspace.ActivationProbe.service"),
        format!(
            "[D-BUS Service]\nName=org.hearthspace.ActivationProbe\nExec={}\n",
            probe.display()
        ),
    )
    .unwrap();
    script(
        &fixture.0.join("hearthspace-compositor"),
        r#"#!/usr/bin/python3
import os, subprocess, time
# The probe deliberately exits without claiming a bus name. Activation still
# records the daemon's environment, and dbus-send then reports the expected error.
subprocess.run(['dbus-send', '--session', '--type=method_call', '--print-reply', '--reply-timeout=200',
    '--dest=org.freedesktop.DBus', '/org/freedesktop/DBus',
    'org.freedesktop.DBus.StartServiceByName',
    'string:org.hearthspace.ActivationProbe', 'uint32:0'],
    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1)
open(os.environ['HEARTHSPACE_HOST_XDG_RUNTIME_DIR'] + '/compositor-started', 'w').close()
while True:
    time.sleep(.1)
"#,
    );
    let runtime = fixture.0.join("runtime");
    let log = fs::File::create(fixture.0.join("session.log")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_hearthspace-session"))
        .args([
            "--installed",
            "--backend",
            "native",
            "--startup-timeout",
            "2",
            "--no-watch-apps",
            "--no-desktop-services",
        ])
        .arg("--binary-dir")
        .arg(&fixture.0)
        .arg("--session-dir")
        .arg(&runtime)
        .env("XDG_RUNTIME_DIR", &fixture.0)
        .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", &fixture.0)
        .env(
            "XDG_DATA_DIRS",
            format!("{}:/usr/share", fixture.0.join("share").display()),
        )
        .env("WAYLAND_DISPLAY", "host-wayland")
        .env("WAYLAND_SOCKET", "123456")
        .env("DISPLAY", ":12345")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/invalid-host-bus")
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    assert!(!bounded_exit(&mut child).success());
    assert!(fixture.0.join("compositor-started").exists());
    let activated = fs::read_to_string(fixture.0.join("activation.json")).unwrap();
    assert!(activated.contains(&format!("\"XDG_RUNTIME_DIR\": \"{}\"", runtime.display())));
    assert!(activated.contains("\"WAYLAND_DISPLAY\": \"wayland-99\""));
    assert!(activated.contains("\"XDG_CURRENT_DESKTOP\": \"Hearthspace\""));
    assert!(activated.contains("\"XDG_SESSION_TYPE\": \"wayland\""));
    assert!(activated.contains("\"DISPLAY\": \"\""));
    assert!(activated.contains("\"WAYLAND_SOCKET\": null"));
    assert!(activated.contains("settings.sock"));
    assert!(activated.contains(&format!(
        "\"PATH\": \"{}:",
        runtime.join("app-bin").display()
    )));
    assert!(!activated.contains("invalid-host-bus"));
    assert!(!runtime.exists());
}

#[test]
fn stalled_bus_activation_cannot_extend_startup_or_leave_the_runtime() {
    let fixture = Fixture::new();
    script(
        &fixture.0.join("dbus-update-activation-environment"),
        "#!/usr/bin/python3\nimport os, time\nopen(os.environ['HEARTHSPACE_HOST_XDG_RUNTIME_DIR'] + '/activation-started', 'w').close()\ntime.sleep(60)\n",
    );
    let started = Instant::now();
    let runtime = fixture.0.join("runtime");
    let mut child = Command::new(env!("CARGO_BIN_EXE_hearthspace-session"))
        .args([
            "--installed",
            "--backend",
            "native",
            "--startup-timeout",
            "1",
            "--no-watch-apps",
            "--no-desktop-services",
        ])
        .arg("--binary-dir")
        .arg(&fixture.0)
        .arg("--session-dir")
        .arg(&runtime)
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.0.display()))
        .env("XDG_RUNTIME_DIR", &fixture.0)
        .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", &fixture.0)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(!bounded_exit(&mut child).success());
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(fixture.0.join("activation-started").exists());
    assert!(!fixture.0.join("compositor-started").exists());
    assert!(!runtime.exists());
}
