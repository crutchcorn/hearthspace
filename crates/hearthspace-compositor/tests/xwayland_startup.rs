//! Direct compositor failures must precede public readiness and preserve stderr.
#![cfg(feature = "xwayland")]

use std::{
    fs,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(feature = "e2e")]
use sha2::{Digest, Sha256};
use smithay::reexports::rustix::process::{
    Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid,
};
#[cfg(feature = "e2e")]
use std::os::unix::fs::PermissionsExt;

struct Fixture {
    root: tempfile::TempDir,
    compositor: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let compositor = root.path().join("hearthspace-compositor");
        let original = env!("CARGO_BIN_EXE_hearthspace-compositor");
        // A hard link preserves the actual executable's companion directory;
        // symlinking would resolve current_exe back into Cargo's target path.
        if fs::hard_link(original, &compositor).is_err() {
            fs::copy(original, &compositor).unwrap();
        }
        Self { root, compositor }
    }

    #[cfg(feature = "e2e")]
    fn candidate(&self, script: &str) {
        let binary = self.root.path().join("hearthspace-Xwayland");
        fs::write(&binary, script).unwrap();
        fs::set_permissions(binary, fs::Permissions::from_mode(0o700)).unwrap();
        let mut receipt: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/xwayland-requirements.json")).unwrap();
        receipt["sha256"] = format!("{:x}", Sha256::digest(script.as_bytes())).into();
        fs::write(
            self.root.path().join("hearthspace-Xwayland.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
    }

    fn failure(&self, expected: &str) -> String {
        self.failure_checked(expected, false)
    }

    fn failure_checked(&self, expected: &str, require_child_reaped: bool) -> String {
        let log_path = self.root.path().join("stderr");
        let mut command = Command::new(&self.compositor);
        command
            .args(["--headless", "--no-shell"])
            .process_group(0)
            .env("XDG_RUNTIME_DIR", self.root.path())
            .env(
                "HEARTHSPACE_COMMAND_SOCKET",
                self.root.path().join("commands"),
            )
            .env("HEARTHSPACE_WAYLAND_DISPLAY", "wayland-startup-test")
            .env("XDG_CONFIG_HOME", self.root.path().join("config"))
            .env("XDG_CACHE_HOME", self.root.path().join("cache"))
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .env("EGL_PLATFORM", "surfaceless")
            .env_remove("HEARTHSPACE_LOG_FILE")
            .env_remove("HEARTHSPACE_LAYOUT_PATH")
            .env_remove("HEARTHSPACE_SESSION_DIR")
            .env_remove("HEARTHSPACE_ACTIVATION_OWNER")
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log_path).unwrap());
        let owned = OwnedChild(command.spawn().unwrap());
        let mut child_identity = None;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if require_child_reaped && child_identity.is_none() {
                child_identity = fs::read_to_string(self.root.path().join("xwayland.pid"))
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok())
                    .and_then(|pid| process_start_time(pid).map(|start| (pid, start)));
            }
            assert!(
                !self.root.path().join("wayland-startup-test").exists(),
                "startup failure exposed public Wayland readiness"
            );
            // Observe exit without reaping: Drop must still own this process
            // group's leader when it signals the group during fixture cleanup.
            if let Some(status) = waitid(
                WaitId::Pid(Pid::from_raw(owned.0.id() as i32).unwrap()),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .unwrap()
            {
                assert!(
                    status.exit_status() != Some(0),
                    "failed dependency must make direct compositor startup fail"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "compositor startup exceeded its bounded failure budget"
            );
            thread::sleep(Duration::from_millis(10));
        }
        if require_child_reaped {
            let (pid, start) = child_identity.expect("observed the live owned Xwayland child");
            assert_ne!(
                process_start_time(pid),
                Some(start),
                "Xwayland child {pid} must be terminated and reaped BEFORE fixture group cleanup"
            );
        }
        let log = fs::read_to_string(log_path).unwrap();
        assert!(log.contains(expected), "{log}");
        assert!(log.contains("hearthspace-Xwayland"), "{log}");
        assert!(log.contains("hearthspace-Xwayland.json"), "{log}");
        assert!(log.contains("normal user"), "{log}");
        log
    }
}

// Retain the observed start time so a recycled PID cannot cause a false leak
// report. Zombies still have /proc entries, so this also checks reaping.
fn process_start_time(pid: u32) -> Option<String> {
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => panic!("cannot inspect owned child's process lifetime: {error}"),
    };
    Some(
        stat.rsplit_once(')')
            .expect("process stat command delimiter")
            .1
            .split_whitespace()
            .nth(19)
            .expect("process stat start time")
            .to_owned(),
    )
}

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // WNOWAIT preserves the leader's identity through this signal. Runtime
        // cleanup assertions run before Drop; this is only a failure safeguard.
        if let Some(pid) = Pid::from_raw(self.0.id() as i32) {
            let _ = kill_process_group(pid, Signal::KILL);
        }
        let _ = self.0.wait();
    }
}

#[test]
fn missing_companion_fails_before_backend_or_public_socket_startup() {
    Fixture::new().failure("No such file");
}

#[cfg(feature = "e2e")]
#[test]
fn early_exit_preserves_child_diagnostics_and_never_omits_required_flags() {
    let fixture = Fixture::new();
    fixture.candidate("#!/bin/sh\nprintf 'fake-child argv: %s\\n' \"$*\" >&2\nexit 17\n");
    let log = fixture.failure("exited during startup");
    assert!(log.contains("fake-child argv:"), "{log}");
    assert!(log.contains("-rootless"), "{log}");
    assert!(log.contains("-surface-input-targets"), "{log}");
}

#[cfg(feature = "e2e")]
#[test]
fn unspawnable_but_correctly_receipted_binary_fails_startup() {
    let fixture = Fixture::new();
    fixture.candidate("#!/no/such/interpreter\n");
    fixture.failure("No such file");
}

#[cfg(feature = "e2e")]
#[test]
fn missing_displayfd_readiness_has_a_direct_compositor_deadline() {
    let fixture = Fixture::new();
    fixture.candidate("#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$XDG_RUNTIME_DIR/xwayland.pid\"\nexec /bin/sleep 90\n");
    fixture.failure_checked(
        "timed out waiting for Xwayland and its window manager",
        true,
    );
}

#[cfg(feature = "e2e")]
#[test]
fn displayfd_readiness_without_a_working_xwm_still_fails_within_deadline() {
    let fixture = Fixture::new();
    fixture.candidate(
        r#"#!/bin/sh
printf '%s\n' "$$" > "$XDG_RUNTIME_DIR/xwayland.pid"
while test "$#" -gt 0; do
    if test "$1" = -displayfd; then shift; displayfd="$1"; fi
    shift
done
printf '17\n' > "/proc/self/fd/$displayfd"
exec /bin/sleep 90
"#,
    );
    fixture.failure_checked("failed to initialize XWM", true);
}
