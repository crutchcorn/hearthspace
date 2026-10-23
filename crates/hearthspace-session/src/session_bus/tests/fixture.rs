use super::*;
use manager::{Manager, State};
use std::{
    fs,
    future::Future,
    io::{BufRead, BufReader},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

pub(super) fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

pub(super) fn run<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

pub(super) fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(Instant::now() < deadline, "fixture readiness timed out");
        thread::sleep(Duration::from_millis(2));
    }
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Directory(PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) struct Bus {
    pub address: String,
    pub state: Arc<Mutex<State>>,
    // Drop connections before terminating the owned daemon, then remove files.
    _manager: zbus::Connection,
    _daemon: Daemon,
    directory: Directory,
    next_probe: AtomicU64,
}

impl Bus {
    pub fn start() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // Deliberately avoid temp_dir(): it reads the test process environment.
        let directory = Directory(PathBuf::from(format!(
            "/tmp/hs-activation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory.0)
            .unwrap();
        // dbus-update checks this canonical-runtime marker before --systemd.
        fs::create_dir(directory.0.join("systemd")).unwrap();
        let services = directory.0.join("services");
        fs::create_dir(&services).unwrap();
        let helper = directory.0.join("probe.py");
        fs::write(&helper, include_str!("probe.py")).unwrap();
        // Each activation has a separate name/output so no daemon cache can
        // turn a post-retirement observation into a previous launch's result.
        for index in 0..8 {
            let name = format!("org.hearthspace.ActivationProbe{index}");
            fs::write(
                services.join(format!("{name}.service")),
                format!(
                    "[D-BUS Service]\nName={name}\nExec=/usr/bin/python3 -I -S {} {} {}\n",
                    helper.display(),
                    directory.0.join(format!("probe-{index}.json")).display(),
                    PROBE_KEYS.join(" ")
                ),
            )
            .unwrap();
        }
        let config = directory.0.join("bus.conf");
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:path={}/bus</listen>\
             <auth>EXTERNAL</auth><servicedir>{}</servicedir>\
             <policy context=\"default\"><allow own=\"*\"/>\
             <allow send_destination=\"*\"/><allow receive_sender=\"*\"/>\
             </policy></busconfig>",
                directory.0.display(),
                services.display()
            ),
        )
        .unwrap();
        let log = fs::File::create(directory.0.join("daemon.log")).unwrap();
        let mut daemon = Daemon(
            Command::new("/usr/bin/dbus-daemon")
                .env_clear()
                .args(["--nofork", "--nopidfile", "--print-address=1"])
                .arg(format!("--config-file={}", config.display()))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(log)
                .spawn()
                .expect("isolated tests require /usr/bin/dbus-daemon"),
        );
        let stdout = daemon.0.stdout.take().unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let mut address = String::new();
            let result = BufReader::new(stdout)
                .read_line(&mut address)
                .map(|_| address);
            let _ = send.send(result);
        });
        let address = receive
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap();
        reader.join().unwrap();
        let address = address.trim().to_owned();
        assert!(address.starts_with("unix:path="));
        let state = Arc::new(Mutex::new(State::default()));
        // async-io's zbus executor runs independently after block_on returns.
        let manager = run(async {
            zbus::connection::Builder::address(address.as_str())
                .unwrap()
                .method_timeout(Duration::from_secs(3))
                .serve_at("/org/freedesktop/systemd1", Manager(state.clone()))
                .unwrap()
                .name("org.freedesktop.systemd1")
                .unwrap()
                .build()
                .await
                .unwrap()
        });
        Self {
            address,
            state,
            _manager: manager,
            _daemon: daemon,
            directory,
            next_probe: AtomicU64::new(0),
        }
    }

    pub fn paths(&self, name: &str) -> SessionPaths {
        let host_runtime_dir = self.directory.0.clone();
        let session_dir = host_runtime_dir.join(name);
        fs::create_dir_all(&session_dir).unwrap();
        SessionPaths {
            settings_socket: session_dir.join("settings.sock"),
            session_socket: session_dir.join("session.sock"),
            shell_command_socket: session_dir.join("command.sock"),
            applications_dir: session_dir.join("share/applications"),
            host_runtime_dir,
            session_dir,
        }
    }

    pub fn session(&self, paths: &SessionPaths) -> SessionEnv {
        let mut vars = pairs(&[
            ("WAYLAND_DISPLAY", "wayland-test"),
            ("XDG_SESSION_TYPE", "wayland"),
            ("XDG_CURRENT_DESKTOP", "Hearthspace"),
            ("XDG_SESSION_DESKTOP", "hearthspace"),
            ("DESKTOP_SESSION", "hearthspace"),
            ("GTK_USE_PORTAL", "1"),
            ("HEARTHSPACE_APP_SCOPE_UNIT", "hearthspace-test.scope"),
            ("HEARTHSPACE_DEV", "1"),
            ("HEARTHSPACE_REPO_ROOT", "/synthetic/repository"),
        ]);
        for (key, path) in [
            ("XDG_RUNTIME_DIR", &paths.session_dir),
            ("HEARTHSPACE_SESSION_DIR", &paths.session_dir),
            ("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", &paths.host_runtime_dir),
            ("HEARTHSPACE_SETTINGS_SOCKET", &paths.settings_socket),
            ("HEARTHSPACE_SESSION_SOCKET", &paths.session_socket),
            ("HEARTHSPACE_COMMAND_SOCKET", &paths.shell_command_socket),
        ] {
            vars.push((key.into(), path.to_str().unwrap().into()));
        }
        for (key, value) in [
            ("HEARTHSPACE_SESSION_BUS_ADDRESS", self.address.clone()),
            ("DBUS_SESSION_BUS_ADDRESS", self.address.clone()),
            (
                "PATH",
                format!("{}/bin:/usr/bin:/bin", paths.session_dir.display()),
            ),
            (
                "XDG_DATA_DIRS",
                format!("{}/share:/usr/share", paths.session_dir.display()),
            ),
            (
                "XDG_CONFIG_DIRS",
                format!("{}/config:/etc/xdg", paths.session_dir.display()),
            ),
        ] {
            vars.push((key.into(), value));
        }
        SessionEnv {
            wayland_display: "wayland-test".into(),
            vars,
        }
    }

    pub fn publish(&self, paths: &SessionPaths) -> ActivationEnvironment {
        update_activation_environment(
            paths,
            &self.session(paths),
            false,
            Instant::now() + Duration::from_secs(5),
            &AtomicBool::new(false),
        )
        .unwrap()
        .expect("shared activation has a retirement guard")
    }

    pub fn values(&self) -> BTreeMap<String, String> {
        self.state.lock().unwrap().values.clone()
    }

    pub fn seed(&self, values: BTreeMap<String, String>) {
        self.state.lock().unwrap().values = values.clone();
        cleanup::operations::update(
            &self.address,
            &values,
            false,
            Instant::now() + Duration::from_secs(3),
            &AtomicBool::new(false),
        )
        .unwrap();
    }

    pub fn probe(&self) -> BTreeMap<String, Option<String>> {
        let index = self.next_probe.fetch_add(1, Ordering::Relaxed);
        assert!(index < 8, "fixture probe services exhausted");
        let name = format!("org.hearthspace.ActivationProbe{index}");
        run(async {
            let connection = zbus::connection::Builder::address(self.address.as_str())
                .unwrap()
                .method_timeout(Duration::from_secs(3))
                .build()
                .await
                .unwrap();
            let proxy = zbus::Proxy::new(
                &connection,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
            )
            .await
            .unwrap();
            // The probe exits after writing, so it deliberately never owns a name.
            let result: zbus::Result<u32> = proxy.call("StartServiceByName", &(name, 0_u32)).await;
            assert!(result.is_err(), "probe unexpectedly acquired a bus name");
        });
        let output = self.directory.0.join(format!("probe-{index}.json"));
        wait_until(|| output.is_file());
        serde_json::from_slice(&fs::read(output).unwrap()).unwrap()
    }
}

pub(super) fn assert_no_session_paths(values: &BTreeMap<String, String>, directory: &Path) {
    let directory = directory.to_str().unwrap();
    for (key, value) in values {
        assert!(!value.contains(directory), "retired path remains in {key}");
    }
}

// Explicit synthetic probe allowlist, including names whose absence is tested.
// The helper never enumerates its environment or touches ordinary profile files.
const PROBE_KEYS: &[&str] = &[
    "XDG_RUNTIME_DIR",
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "PATH",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_DIRS",
    "GTK_USE_PORTAL",
    "HEARTHSPACE_SESSION_DIR",
    "HEARTHSPACE_HOST_XDG_RUNTIME_DIR",
    "HEARTHSPACE_SESSION_BUS_ADDRESS",
    "HEARTHSPACE_SETTINGS_SOCKET",
    "HEARTHSPACE_SESSION_SOCKET",
    "HEARTHSPACE_COMMAND_SOCKET",
    "HEARTHSPACE_APP_SCOPE_UNIT",
    "HEARTHSPACE_DEV",
    "HEARTHSPACE_REPO_ROOT",
    "HEARTHSPACE_ACTIVATION_OWNER",
    "AT_SPI_BUS_ADDRESS",
    "WAYLAND_SOCKET",
    "RUST_LOG",
    "ARBITRARY_TEST_VALUE",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "GSETTINGS_BACKEND",
];
