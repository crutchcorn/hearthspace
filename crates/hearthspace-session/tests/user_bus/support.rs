use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);
pub const ROOT_ENV: &str = "HEARTHSPACE_TEST_USER_BUS_ROOT";
pub const ROLE_ENV: &str = "HEARTHSPACE_TEST_USER_BUS_ROLE";

pub struct Fixture {
    pub root: PathBuf,
    pub host: PathBuf,
    pub runtime: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        // Keep Unix socket paths short, like the canonical /run/user/UID runtime.
        let root = PathBuf::from(format!(
            "/tmp/hs-user-bus-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let fixture = Self {
            host: root.join("host-runtime"),
            runtime: root.join("owned-session"),
            root,
        };
        for directory in ["host-runtime", "supplied", "bin", "home", "data", "config"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(fixture.root.join(directory))
                .unwrap();
        }
        for role in ["settingsd", "compositor"] {
            script(
                &fixture.root.join(format!("bin/hearthspace-{role}")),
                &format!(
                    r#"#!/usr/bin/python3
import ctypes, os, signal, sys
# The supervisor gives each child its own process group. Also terminate this
# fixture if a failed assertion forces the parent to kill the supervisor.
parent = os.getppid()
assert ctypes.CDLL(None).prctl(1, signal.SIGKILL, 0, 0, 0) == 0
if os.getppid() != parent:
    sys.exit(98)
role = '{role}'
if role == 'compositor':
    assert '--tty' in sys.argv
os.environ['{ROLE_ENV}'] = role
probe = os.environ['HEARTHSPACE_TEST_USER_BUS_EXE']
os.execv(probe, [probe, '--exact', 'user_bus_child_probe', '--nocapture'])
"#
                ),
            );
        }
        // Fail closed if startup regresses into launching another broker or
        // publishing activation. The fixtures themselves use absolute paths.
        for helper in [
            "dbus-run-session",
            "dbus-daemon",
            "dbus-broker-launch",
            "hearthspace-session-bus",
            "dbus-update-activation-environment",
            "systemctl",
            "systemd-run",
        ] {
            script(
                &fixture.root.join("bin").join(helper),
                &format!(
                    "#!/usr/bin/python3\nimport os, sys\nwith open(os.environ['{ROOT_ENV}'] + '/unexpected-helper', 'a') as report:\n    report.write(sys.argv[0] + '\\n')\nsys.exit(97)\n"
                ),
            );
        }
        fixture
    }

    pub fn canonical_address(&self) -> String {
        address(&self.host.join("bus"))
    }

    pub async fn run(&self, supplied_address: &str, private: bool) {
        assert!(!self.host.join("systemd/private").exists());
        let mut command = Command::new(env!("CARGO_BIN_EXE_hearthspace-session"));
        command
            .env_clear()
            .args([
                "--installed",
                "--backend",
                "native",
                "--startup-timeout",
                "10",
                "--no-watch-apps",
                "--no-desktop-services",
                "--no-settingsd-restart",
            ])
            .arg("--binary-dir")
            .arg(self.root.join("bin"))
            .arg("--session-dir")
            .arg(&self.runtime)
            .env(ROOT_ENV, &self.root)
            .env(
                "HEARTHSPACE_TEST_USER_BUS_EXE",
                std::env::current_exe().unwrap(),
            )
            .env(
                "HEARTHSPACE_TEST_USER_BUS_CANONICAL",
                self.canonical_address(),
            )
            .env("HEARTHSPACE_TEST_USER_BUS_SUPPLIED", supplied_address)
            .env("XDG_RUNTIME_DIR", &self.host)
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", &self.host)
            .env("DBUS_SESSION_BUS_ADDRESS", supplied_address)
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                address(&self.root.join("no-system-bus")),
            )
            .env("HOME", self.root.join("home"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_DATA_DIRS", self.root.join("data"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_CONFIG_DIRS", self.root.join("config"))
            .env("TMPDIR", &self.root)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .env("DISPLAY", ":12345")
            .env("WAYLAND_DISPLAY", "other-desktop")
            .env("WAYLAND_SOCKET", "123456");
        if private {
            command.arg("--private-session-bus");
        }
        let mut supervisor = OwnedProcess::spawn(command, self.root.join("supervisor.log"));
        let status = supervisor.wait_for(Duration::from_secs(15)).await;
        let log = supervisor.log();
        assert!(status.is_some_and(|status| !status.success()), "{log}");
        assert!(log.contains("compositor exited before readiness"), "{log}");
        assert!(
            !self.runtime.exists(),
            "startup did not clean its runtime: {log}"
        );
        assert!(
            !self.root.join("unexpected-helper").exists(),
            "unexpected helper: {}\n{log}",
            fs::read_to_string(self.root.join("unexpected-helper")).unwrap_or_default()
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn script(path: &Path, source: &str) {
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn address(socket: &Path) -> String {
    // All other bytes in our owned paths are D-Bus address-safe ASCII.
    format!("unix:path={}", socket.to_str().unwrap().replace(' ', "%20"))
}

pub struct Broker {
    process: OwnedProcess,
    pub address: String,
}

impl Broker {
    pub fn start(directory: &Path) -> Self {
        let config = directory.join("bus.conf");
        let listen = address(&directory.join("bus"));
        // No includes or service directories: these brokers cannot activate
        // the host keyring, user manager, or any other real desktop services.
        fs::write(
            &config,
            format!(
                r#"<busconfig>
  <type>session</type>
  <listen>{listen}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow user="*"/>
    <allow own="*"/>
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
  </policy>
</busconfig>
"#
            ),
        )
        .unwrap();
        let output = directory.join("address");
        let mut command = Command::new("/usr/bin/dbus-daemon");
        command
            .env_clear()
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .arg(format!("--config-file={}", config.display()));
        let mut process = OwnedProcess::spawn(command, output.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        let address = loop {
            assert!(process.poll().is_none(), "broker exited: {}", process.log());
            let contents = fs::read_to_string(&output).unwrap_or_default();
            if let Some(line) = contents.lines().find(|line| line.starts_with("unix:"))
                && contents.ends_with('\n')
            {
                break line.to_owned();
            }
            assert!(
                Instant::now() < deadline,
                "broker timed out: {}",
                process.log()
            );
            thread::sleep(Duration::from_millis(10));
        };
        Self { process, address }
    }

    pub fn assert_running(&mut self) {
        assert!(
            self.process.poll().is_none(),
            "supervisor stopped a borrowed bus: {}",
            self.process.log()
        );
    }
}

struct OwnedProcess {
    child: Child,
    status: Option<ExitStatus>,
    output: PathBuf,
}

impl OwnedProcess {
    fn spawn(mut command: Command, output: PathBuf) -> Self {
        let log = fs::File::create(&output).unwrap();
        let child = command
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        Self {
            child,
            status: None,
            output,
        }
    }

    fn poll(&mut self) -> Option<ExitStatus> {
        if self.status.is_none() {
            self.status = self.child.try_wait().unwrap();
        }
        self.status
    }

    async fn wait_for(&mut self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.poll() {
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.output).unwrap_or_default()
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if self.status.is_some() {
            return;
        }
        // The child is unreaped, so this PID cannot have been reused. Never
        // perform an unbounded wait(), including while unwinding an assertion.
        let _ = kill(Pid::from_raw(self.child.id() as i32), Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        eprintln!(
            "fixture child {} did not reap after SIGKILL",
            self.child.id()
        );
    }
}
