//! Real cleanup callers against an isolated systemctl process boundary. No host
//! manager, desktop, GPU, or service mutations participate in these tests.
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

const OWNER: &str = "hearthspace-apps-session.service";
const RECORDED: &str = "snap.test.recorded.scope";
const BOUND: &str = "ptyxis-spawn-01234567-89ab-cdef-0123-456789abcdef.scope";

struct Fixture {
    root: PathBuf,
    runtime: PathBuf,
    child: Option<Child>,
    deadline: Instant,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(format!(
            "/tmp/hs-scope-cleanup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for name in ["host", "bin", "home", "config", "data", "cache", "state"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        let manager = root.join("manager.py");
        fs::write(&manager, include_str!("app_scope_cleanup/manager.py")).unwrap();
        fs::set_permissions(&manager, fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["systemctl", "hearthspace-settingsd"] {
            symlink(&manager, root.join("bin").join(name)).unwrap();
        }
        Self {
            runtime: root.join("session"),
            root,
            child: None,
            deadline: Instant::now() + Duration::from_secs(10),
        }
    }

    fn record(&self) {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&self.runtime)
            .unwrap();
        fs::write(self.runtime.join("app-scope-owner"), OWNER).unwrap();
        fs::create_dir(self.runtime.join("app-scopes")).unwrap();
        fs::write(self.runtime.join("app-scopes").join(RECORDED), "").unwrap();
    }

    fn spawn(&mut self, helper: bool) {
        let binary = if helper {
            env!("CARGO_BIN_EXE_hearthspace-app-scope-guard")
        } else {
            env!("CARGO_BIN_EXE_hearthspace-session")
        };
        let mut command = Command::new(binary);
        command
            .env_clear()
            .current_dir(&self.root)
            .env("HEARTHSPACE_SCOPE_TEST_ROOT", &self.root)
            .env("HOME", self.root.join("home"))
            .env("XDG_RUNTIME_DIR", self.root.join("host"))
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", self.root.join("host"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_CONFIG_DIRS", self.root.join("config"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_DATA_DIRS", self.root.join("data"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/nonexistent/scope-test-bus",
            )
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                "unix:path=/nonexistent/scope-test-system",
            )
            .env("PATH", self.root.join("bin"));
        if helper {
            command
                .arg("--stop-app-scopes")
                .arg(&self.runtime)
                .arg(self.root.join("host"));
        } else {
            command
                .args([
                    "--installed",
                    "--backend",
                    "nested",
                    "--no-desktop-services",
                    "--no-watch-apps",
                    "--no-settingsd-restart",
                    "--startup-timeout",
                    "2",
                ])
                .arg("--binary-dir")
                .arg(self.root.join("bin"))
                .arg("--session-dir")
                .arg(&self.runtime);
        }
        let log = fs::File::create(self.root.join("output")).unwrap();
        self.child = Some(
            command
                .stdin(Stdio::null())
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
    }

    fn observed(&mut self, scope: &str) {
        while !self.root.join(format!("observed-{scope}")).exists() {
            assert!(
                self.child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "cleanup exited before observing the live scope: {}",
                self.output()
            );
            self.tick();
        }
        assert!(self.child.as_mut().unwrap().try_wait().unwrap().is_none());
        assert!(self.runtime.join("app-scope-owner").exists());
        assert!(self.runtime.join("app-scopes").join(RECORDED).exists());
    }

    fn release(&self, scope: &str) {
        fs::write(self.root.join(format!("release-{scope}")), "").unwrap();
    }

    fn wait(&mut self) -> ExitStatus {
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                return status;
            }
            self.tick();
        }
    }

    fn tick(&self) {
        assert!(
            Instant::now() < self.deadline,
            "cleanup fixture timed out: {}",
            self.output()
        );
        thread::sleep(Duration::from_millis(10));
    }

    fn output(&self) -> String {
        fs::read_to_string(self.root.join("output")).unwrap_or_default()
    }

    fn commands(&self) -> String {
        fs::read_to_string(self.root.join("commands")).unwrap_or_default()
    }

    fn assert_records_retained(&self) {
        assert_eq!(
            fs::read_to_string(self.runtime.join("app-scope-owner")).unwrap(),
            OWNER
        );
        assert!(self.runtime.join("app-scopes").join(RECORDED).is_file());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            // A failing assertion also has a bounded teardown.
            let deadline = Instant::now() + Duration::from_secs(2);
            while child.try_wait().is_ok_and(|status| status.is_none()) && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(10));
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn cleanup_helper_waits_for_recorded_and_bound_scopes_after_owner_stop() {
    let mut fixture = Fixture::new();
    fixture.record();
    fixture.spawn(true);
    fixture.observed(RECORDED);
    fixture.observed(BOUND);
    assert!(fixture.root.join("owner-stopped").exists());
    fixture.release(RECORDED);
    assert!(
        fixture
            .child
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_none()
    );
    fixture.release(BOUND);
    assert!(fixture.wait().success(), "{}", fixture.output());
    // Only the caller may remove the runtime after the stop barrier succeeds.
    fixture.assert_records_retained();
}

#[test]
fn collected_owner_still_waits_for_recorded_scope() {
    let mut fixture = Fixture::new();
    fixture.record();
    fs::write(fixture.root.join("owner-collected"), "").unwrap();
    fixture.spawn(true);
    fixture.observed(RECORDED);
    fixture.release(RECORDED);
    assert!(fixture.wait().success(), "{}", fixture.output());
    assert!(!fixture.commands().contains("stop "));
}

#[test]
fn supervisor_shutdown_uses_same_barrier_after_partial_start() {
    let mut fixture = Fixture::new();
    fixture.spawn(false);
    fixture.observed(RECORDED);
    fixture.observed(BOUND);
    fixture.release(RECORDED);
    fixture.release(BOUND);
    // Settings startup failed, but normal shutdown still awaited app scopes.
    assert!(!fixture.wait().success());
    assert!(!fixture.runtime.exists(), "{}", fixture.output());
}

#[test]
fn cleanup_helper_retains_records_when_stop_fails() {
    let mut fixture = Fixture::new();
    fixture.record();
    fs::write(fixture.root.join("stop-fails"), "").unwrap();
    fixture.spawn(true);
    assert!(!fixture.wait().success());
    assert!(fixture.output().contains("fixture stop failed"));
    fixture.assert_records_retained();
}

#[test]
fn supervisor_retains_runtime_when_stop_fails() {
    let mut fixture = Fixture::new();
    fs::write(fixture.root.join("stop-fails"), "").unwrap();
    fixture.spawn(false);
    assert!(!fixture.wait().success());
    assert!(fixture.commands().contains(&format!("stop {OWNER}")));
    fixture.assert_records_retained();
}

#[test]
fn cleanup_deadline_retains_records_when_scope_never_stops() {
    let mut fixture = Fixture::new();
    fixture.record();
    fixture.spawn(true);
    fixture.observed(RECORDED);
    assert!(!fixture.wait().success());
    assert!(
        fixture.output().contains("deadline"),
        "{}",
        fixture.output()
    );
    fixture.assert_records_retained();
}

#[test]
fn changed_owner_identity_never_receives_stop() {
    let mut fixture = Fixture::new();
    fixture.record();
    fs::write(fixture.root.join("foreign-owner"), "").unwrap();
    fixture.spawn(true);
    assert!(!fixture.wait().success());
    assert!(fixture.output().contains("identity changed"));
    assert!(!fixture.commands().contains("stop "));
    fixture.assert_records_retained();
}

#[test]
fn foreign_owner_intent_is_rejected_before_manager_contact() {
    let mut fixture = Fixture::new();
    fixture.record();
    fs::write(fixture.runtime.join("app-scope-owner"), "other.service").unwrap();
    fixture.spawn(true);
    assert!(!fixture.wait().success());
    assert!(fixture.output().contains("intent does not match"));
    assert!(fixture.commands().is_empty());
    assert!(fixture.runtime.exists());
}
