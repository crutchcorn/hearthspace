//! Installed login defaults must tolerate retired activation-environment values.
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "hearthspace-login-paths-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for name in ["host", "inherited-runtime", "home"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(name))
                .unwrap();
        }
        Self(root)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hearthspace-session"));
        // Match the installed entry's mode, including its default session path.
        // Dry-run prepares actual environment/config files without any services,
        // compositor, login guard, or access to the real user's bus/profile.
        command
            .args([
                "--installed",
                "--backend",
                "native",
                "--login-session",
                "--no-watch-apps",
                "--dry-run",
            ])
            .arg("--binary-dir")
            .arg(&self.0)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.0.join("home"))
            .env("XDG_RUNTIME_DIR", self.0.join("host"))
            .env("GDMSESSION", "hearthspace")
            .current_dir(&self.0)
            .stdin(Stdio::null());
        command
    }

    fn run(&self, command: &mut Command, success: bool) -> BTreeMap<String, String> {
        let stdout = self.0.join("stdout");
        let stderr = self.0.join("stderr");
        let mut child = command
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("installed login preparation timed out");
            }
            thread::sleep(Duration::from_millis(10));
        };
        let errors = fs::read_to_string(stderr).unwrap();
        assert_eq!(status.success(), success, "{errors}");
        if !success {
            assert!(errors.contains("runtime directories must be absolute paths"));
            return BTreeMap::new();
        }
        fs::read_to_string(stdout)
            .unwrap()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect()
    }

    fn assert_host(&self, values: &BTreeMap<String, String>) {
        let host = self.0.join("host");
        assert_eq!(
            values["HEARTHSPACE_HOST_XDG_RUNTIME_DIR"],
            host.to_str().unwrap()
        );
        let runtime = PathBuf::from(&values["XDG_RUNTIME_DIR"]);
        assert_eq!(runtime.parent(), Some(host.join("hearthspace").as_path()));
        assert!(
            runtime
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("session-")
        );
        assert_eq!(values["HEARTHSPACE_SESSION_DIR"], runtime.to_str().unwrap());
        assert_eq!(
            fs::metadata(&runtime).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(
            runtime
                .join("share/applications/dev.hearthspace.Settings.desktop")
                .is_file()
        );
        assert_eq!(
            values["DBUS_SESSION_BUS_ADDRESS"],
            format!("unix:path={}/bus", host.display())
        );
        assert!(!runtime.join("processes.json").exists());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn installed_login_ignores_empty_retired_host_runtime_override() {
    let fixture = Fixture::new();
    let values = fixture.run(
        fixture
            .command()
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", ""),
        true,
    );
    fixture.assert_host(&values);
}

#[test]
fn installed_login_without_override_allocates_under_xdg_runtime() {
    let fixture = Fixture::new();
    let values = fixture.run(&mut fixture.command(), true);
    fixture.assert_host(&values);
}

#[test]
fn installed_login_preserves_explicit_nonempty_host_runtime() {
    let fixture = Fixture::new();
    let values = fixture.run(
        fixture
            .command()
            .env("XDG_RUNTIME_DIR", fixture.0.join("inherited-runtime"))
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", fixture.0.join("host")),
        true,
    );
    fixture.assert_host(&values);
    assert!(
        fs::read_dir(fixture.0.join("inherited-runtime"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn installed_login_still_rejects_relative_host_override() {
    let fixture = Fixture::new();
    fixture.run(
        fixture
            .command()
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", "relative"),
        false,
    );
    assert!(!fixture.0.join("relative").exists());
    assert!(!fixture.0.join("host/hearthspace").exists());
}

#[test]
fn installed_login_still_rejects_empty_xdg_without_valid_override() {
    let fixture = Fixture::new();
    fixture.run(
        fixture
            .command()
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", "")
            .env("XDG_RUNTIME_DIR", ""),
        false,
    );
    assert!(!fixture.0.join("hearthspace").exists());
}
