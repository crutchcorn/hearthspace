use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
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
use serde_json::Value;

pub struct Fixture {
    pub root: PathBuf,
    pub runtime: PathBuf,
    pub host_address: String,
    supervisor: Option<Owned>,
    host: Option<Owned>,
}

impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(format!(
            "/tmp/hs-dev-bus-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for name in ["bin", "host", "home", "data", "config"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(name))
                .unwrap();
        }
        script(&root.join("cargo.py"), include_str!("child.py"));
        symlink(root.join("cargo.py"), root.join("bin/cargo")).unwrap();
        // No real user-manager commands may escape this fixture. The canonical
        // runtime deliberately has no systemd/private socket.
        for name in ["systemctl", "systemd-run"] {
            script(
                &root.join("bin").join(name),
                "#!/usr/bin/python3 -I\nimport os, pathlib, sys\npathlib.Path(os.environ['HEARTHSPACE_TEST_DEV_BUS_ROOT'], 'unexpected-manager').write_text(sys.argv[0])\nsys.exit(99)\n",
            );
        }
        let services = root.join("data/dbus-1/services");
        fs::create_dir_all(&services).unwrap();
        let probe = root.join("activation-probe");
        script(
            &probe,
            r#"#!/usr/bin/python3 -I
import os
os.environ['HEARTHSPACE_TEST_DEV_BUS_ACTIVATE'] = '1'
executable = os.environ['HEARTHSPACE_TEST_DEV_BUS_EXE']
os.execv(executable, [executable, '--exact', 'development_activation_probe', '--nocapture'])
"#,
        );
        fs::write(
            services.join("org.hearthspace.DevBusProbe.service"),
            format!(
                "[D-BUS Service]\nName=org.hearthspace.DevBusProbe\nExec={}\n",
                probe.display()
            ),
        )
        .unwrap();
        let host_address = format!("unix:path={}/host/bus", root.display());
        let config = root.join("host/bus.conf");
        fs::write(&config, format!(r#"<busconfig>
<type>session</type><listen>{host_address}</listen><auth>EXTERNAL</auth>
<servicedir>{}</servicedir>
<policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy>
</busconfig>"#, services.display())).unwrap();
        let mut command = Command::new("/usr/bin/dbus-daemon");
        command
            .env_clear()
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .arg(format!("--config-file={}", config.display()))
            .env("HEARTHSPACE_TEST_DEV_BUS_ROOT", &root)
            .env(
                "HEARTHSPACE_TEST_DEV_BUS_EXE",
                std::env::current_exe().unwrap(),
            )
            .env("XDG_RUNTIME_DIR", root.join("host"))
            .env("WAYLAND_DISPLAY", "fixture-host")
            .env("DISPLAY", ":777")
            .env("WAYLAND_SOCKET", "123456")
            .env("AT_SPI_BUS_ADDRESS", "unix:path=/fixture-host-a11y")
            .env("XDG_CURRENT_DESKTOP", "FixtureHost");
        let mut host = Owned::spawn(command, root.join("host.log"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("host/bus").exists() {
            assert!(host.poll().is_none(), "{}", host.log());
            assert!(Instant::now() < deadline, "{}", host.log());
            thread::sleep(Duration::from_millis(10));
        }
        Self {
            runtime: root.join("runtime"),
            root,
            host_address,
            supervisor: None,
            host: Some(host),
        }
    }

    pub fn start(&mut self, mode: &str, extra: &[&str]) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hearthspace-session"));
        command
            .env_clear()
            .args([
                "--dev",
                "--backend",
                "native",
                "--no-watch-apps",
                "--startup-timeout",
                "8",
            ])
            .args(extra)
            .arg("--session-dir")
            .arg(&self.runtime)
            .env("HEARTHSPACE_TEST_DEV_BUS_ROOT", &self.root)
            .env("HEARTHSPACE_TEST_DEV_BUS_MODE", mode)
            .env(
                "HEARTHSPACE_TEST_DEV_BUS_EXE",
                std::env::current_exe().unwrap(),
            )
            .env("HOME", self.root.join("home"))
            .env("XDG_RUNTIME_DIR", self.root.join("host"))
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", self.root.join("host"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_DATA_DIRS", self.root.join("data"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_CONFIG_DIRS", self.root.join("config"))
            .env("DBUS_SESSION_BUS_ADDRESS", &self.host_address)
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                "unix:path=/no-fixture-system-bus",
            )
            .env("DISPLAY", ":777")
            .env("WAYLAND_DISPLAY", "fixture-host")
            .env("WAYLAND_SOCKET", "123456")
            .env("AT_SPI_BUS_ADDRESS", "unix:path=/fixture-host-a11y")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            )
            .current_dir(&self.root);
        if mode != "desktop-dry-run" {
            command.arg("--no-desktop-services");
        }
        self.supervisor = Some(Owned::spawn(command, self.root.join("supervisor.log")));
    }

    pub fn wait_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let supervisor = self.supervisor.as_mut().unwrap();
            assert!(supervisor.poll().is_none(), "{}", supervisor.log());
            if supervisor
                .log()
                .contains("monitoring Hearthspace session processes")
            {
                return;
            }
            assert!(Instant::now() < deadline, "{}", supervisor.log());
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn wait_file(&mut self, path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !path.exists() {
            let supervisor = self.supervisor.as_mut().unwrap();
            assert!(supervisor.poll().is_none(), "{}", supervisor.log());
            assert!(Instant::now() < deadline, "{}", supervisor.log());
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn record(&self) -> Value {
        serde_json::from_slice(&fs::read(self.runtime.join("processes.json")).unwrap()).unwrap()
    }

    pub fn signal(&mut self, signal: Signal) {
        let supervisor = self.supervisor.as_mut().unwrap();
        assert!(supervisor.poll().is_none());
        kill(Pid::from_raw(supervisor.child.id() as i32), signal).unwrap();
    }

    pub fn wait_exit(&mut self) -> ExitStatus {
        self.supervisor
            .as_mut()
            .unwrap()
            .wait(Duration::from_secs(12))
    }

    pub fn assert_host_alive(&mut self) {
        assert!(self.host.as_mut().unwrap().poll().is_none());
        assert!(!self.root.join("unexpected-manager").exists());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.supervisor.take();
        self.host.take();
        if thread::panicking() {
            eprintln!(
                "Keeping failed development bus fixture at {}",
                self.root.display()
            );
        } else {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

pub fn assert_dead(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let alive = fs::read_to_string(format!("/proc/{pid}/stat"))
            .is_ok_and(|stat| !stat.rsplit_once(") ").unwrap().1.starts_with('Z'));
        if !alive {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "owned child {pid} survived shutdown"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn script(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

struct Owned {
    child: Child,
    status: Option<ExitStatus>,
    output: PathBuf,
}

impl Owned {
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
    fn log(&self) -> String {
        fs::read_to_string(&self.output).unwrap_or_default()
    }
    fn wait(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.poll() {
                return status;
            }
            assert!(Instant::now() < deadline, "{}", self.log());
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if self.status.is_some() {
            return;
        }
        let _ = kill(Pid::from_raw(self.child.id() as i32), Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(3);
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
