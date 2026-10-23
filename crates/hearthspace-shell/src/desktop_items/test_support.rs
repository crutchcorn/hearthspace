//! Isolated process environments keep GIO's caches and desktop services private.
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub const CHILD_ROOT: &str = "HEARTHSPACE_TEST_FILE_ACTION_ROOT";

pub struct Fixture(pub tempfile::TempDir);

impl Fixture {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for directory in ["home", "config", "share/applications", "runtime"] {
            fs::create_dir_all(root.path().join(directory)).unwrap();
        }
        // Use the installed MIME database without exposing installed launchers.
        symlink("/usr/share/mime", root.path().join("share/mime")).unwrap();
        Self(root)
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }

    pub fn child(&self, test: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", test, "--nocapture"])
            .env_clear()
            .env(CHILD_ROOT, self.path())
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.path().join("home"))
            .env("XDG_CONFIG_HOME", self.path().join("config"))
            .env("XDG_CONFIG_DIRS", self.path().join("config"))
            .env("XDG_DATA_HOME", self.path().join("share"))
            .env("XDG_DATA_DIRS", self.path().join("share"))
            .env("XDG_RUNTIME_DIR", self.path().join("runtime"))
            .env("XDG_CURRENT_DESKTOP", "Hearthspace")
            .env("GIO_USE_VFS", "local")
            .env("DISPLAY", ":host-must-not-be-used")
            .env("WAYLAND_DISPLAY", "host-must-not-be-used")
            .env("HEARTHSPACE_WAYLAND_DISPLAY", "wayland-file-action-test")
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/nonexistent-host-bus",
            )
            .env(
                "HEARTHSPACE_SESSION_BUS_ADDRESS",
                "unix:path=/nonexistent-private-bus",
            )
            .current_dir(self.path());
        command
    }
}

pub fn child_root() -> Option<PathBuf> {
    std::env::var_os(CHILD_ROOT).map(PathBuf::from)
}

pub fn assert_success(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "isolated file action failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub struct Bus {
    child: Child,
    pub address: String,
}

impl Bus {
    pub fn start(root: &Path) -> Self {
        let config = root.join("bus.conf");
        // No service directories: a missing peer can never activate a host app.
        fs::write(&config, "<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>").unwrap();
        let mut child = Command::new("dbus-daemon")
            .arg("--config-file")
            .arg(config)
            .args(["--nofork", "--print-address=1", "--nopidfile"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is required for file-manager protocol tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().into(),
        }
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
