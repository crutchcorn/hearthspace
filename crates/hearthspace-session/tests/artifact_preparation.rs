//! Run the actual supervisor through helper preparation with disposable Cargo
//! and systemd command boundaries. No real compilation, bus, service or VT.
use std::{
    fs,
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt, symlink},
        net::UnixListener,
    },
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

struct Fixture {
    root: PathBuf,
    artifact: PathBuf,
    child: Option<Child>,
    _manager: UnixListener,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(format!(
            "/tmp/hs-artifacts-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for directory in [
            "bin",
            "host",
            "host/systemd",
            "home",
            "data",
            "config",
            "cache",
            "state",
        ] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(directory))
                .unwrap();
        }
        let manager = UnixListener::bind(root.join("host/systemd/private")).unwrap();
        let script = root.join("child.py");
        fs::write(&script, include_str!("artifact_preparation/child.py")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        for program in ["cargo", "systemd-run", "systemctl", "hearthspace-settingsd"] {
            symlink(&script, root.join("bin").join(program)).unwrap();
        }
        // No built sibling helper can accidentally make a clean-start test pass.
        let source = env!("CARGO_BIN_EXE_hearthspace-session");
        let destination = root.join("bin/hearthspace-session");
        fs::hard_link(source, &destination)
            .or_else(|_| fs::copy(source, &destination).map(|_| ()))
            .unwrap();
        Self {
            artifact: root.join("custom target/debug/hearthspace-app-scope-guard"),
            root,
            child: None,
            _manager: manager,
        }
    }

    fn stale_sibling(&mut self) {
        self.artifact = self.root.join("bin/hearthspace-app-scope-guard");
        fs::write(&self.artifact, "#!/bin/sh\n# stale helper\nexit 0\n").unwrap();
        fs::set_permissions(&self.artifact, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn start(&mut self, development: bool, mode: &str) {
        let mut command = Command::new(self.root.join("bin/hearthspace-session"));
        command
            .env_clear()
            .env("HEARTHSPACE_ARTIFACT_TEST_ROOT", &self.root)
            .env("ARTIFACT_PATH", &self.artifact)
            .env("ARTIFACT_MODE", mode)
            .env("CARGO_TARGET_DIR", self.root.join("custom target"))
            .env("HEARTHSPACE_REPO_ROOT", "/nonexistent/installed-checkout")
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
                "DBUS_SYSTEM_BUS_ADDRESS",
                "unix:path=/nonexistent/artifact-test-system",
            )
            .env("PATH", self.root.join("bin"))
            .current_dir(&self.root)
            .args([
                "--backend",
                "native",
                "--no-desktop-services",
                "--no-watch-apps",
                "--startup-timeout",
                if mode == "hang" { "1" } else { "5" },
            ])
            .arg("--session-dir")
            .arg(self.root.join("session"));
        if development {
            command.arg("--dev");
        } else {
            command
                .arg("--installed")
                .arg("--binary-dir")
                .arg(self.root.join("bin"));
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

    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(9);
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "supervisor did not finish: {}",
                self.output()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn output(&self) -> String {
        fs::read_to_string(self.root.join("output")).unwrap_or_default()
    }

    fn submitted(&self) -> serde_json::Value {
        serde_json::from_slice(&fs::read(self.root.join("owner-submitted")).unwrap()).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
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
fn clean_dev_start_prepares_guard_at_cargos_custom_artifact_path() {
    let mut fixture = Fixture::new();
    fixture.start(true, "fresh");
    assert!(
        !fixture.wait().success(),
        "fixture must end before native startup"
    );
    let submitted = fixture.submitted();
    assert_eq!(submitted["helper"].as_str(), fixture.artifact.to_str());
    assert!(
        submitted["contents"]
            .as_str()
            .unwrap()
            .contains("fresh helper")
    );
    assert!(fixture.root.join("build-called").exists());
    assert!(
        !fixture.root.join("session").exists(),
        "{}",
        fixture.output()
    );
}

#[test]
fn existing_sibling_guard_is_refreshed_by_cargo_before_owner_submission() {
    let mut fixture = Fixture::new();
    fixture.stale_sibling();
    fixture.start(true, "fresh");
    assert!(!fixture.wait().success());
    assert!(
        fixture.submitted()["contents"]
            .as_str()
            .unwrap()
            .contains("fresh helper")
    );
    assert!(fixture.root.join("build-called").exists());
}

#[test]
fn failed_guard_build_never_launches_a_stale_sibling() {
    let mut fixture = Fixture::new();
    fixture.stale_sibling();
    fixture.start(true, "fail");
    assert!(!fixture.wait().success());
    assert!(
        fixture.output().contains("build failed"),
        "{}",
        fixture.output()
    );
    assert!(!fixture.root.join("owner-submitted").exists());
    assert!(!fixture.root.join("session").exists());
}

#[test]
fn missing_cargo_artifact_message_never_launches_a_stale_sibling() {
    let mut fixture = Fixture::new();
    fixture.stale_sibling();
    fixture.start(true, "no-message");
    assert!(!fixture.wait().success());
    assert!(
        fixture.output().contains("did not report one executable"),
        "{}",
        fixture.output()
    );
    assert!(!fixture.root.join("owner-submitted").exists());
}

#[test]
fn missing_reported_guard_prevents_owner_submission() {
    let mut fixture = Fixture::new();
    fixture.start(true, "missing");
    assert!(!fixture.wait().success());
    assert!(
        fixture.output().contains("artifact is unavailable"),
        "{}",
        fixture.output()
    );
    assert!(!fixture.root.join("owner-submitted").exists());
}

#[test]
fn installed_mode_uses_release_sibling_without_consulting_cargo() {
    let mut fixture = Fixture::new();
    fixture.stale_sibling();
    fixture.start(false, "fail");
    assert!(!fixture.wait().success());
    assert_eq!(
        fixture.submitted()["helper"].as_str(),
        fixture.artifact.to_str()
    );
    assert!(!fixture.root.join("build-called").exists());
}

#[test]
fn stalled_guard_build_cannot_outlive_startup_deadline() {
    let mut fixture = Fixture::new();
    fixture.start(true, "hang");
    assert!(!fixture.wait().success());
    assert!(
        fixture.output().contains("deadline"),
        "{}",
        fixture.output()
    );
    let pid = fs::read_to_string(fixture.root.join("build-pid")).unwrap();
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
    assert!(!fixture.root.join("owner-submitted").exists());
    assert!(!fixture.root.join("session").exists());
}
