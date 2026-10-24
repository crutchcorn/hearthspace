use std::{
    collections::BTreeMap,
    fs,
    future::Future,
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::manager::{Manager, State};

const PROFILE_DIRS: &[&str] = &["home", "config", "cache", "data", "state"];

pub struct Fixture {
    pub root: PathBuf,
    pub host: PathBuf,
    pub runtime: PathBuf,
    pub state: Arc<Mutex<State>>,
    pub previous: BTreeMap<String, String>,
    manager: Option<zbus::Connection>,
    children: Vec<(Child, PathBuf)>,
    work_deadline: Instant,
    cleanup_deadline: Instant,
}

impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let started = Instant::now();
        // Do not consult TMPDIR, HOME, or any other real process environment.
        let root = PathBuf::from(format!(
            "/tmp/hs-publish-failure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let mut fixture = Self {
            host: root.join("host"),
            runtime: root.join("session"),
            root,
            state: Arc::new(Mutex::new(State::default())),
            previous: BTreeMap::new(),
            manager: None,
            children: Vec::new(),
            work_deadline: started + Duration::from_secs(11),
            cleanup_deadline: started + Duration::from_secs(14),
        };
        for directory in
            PROFILE_DIRS
                .iter()
                .copied()
                .chain(["host", "host/systemd", "bin", "services"])
        {
            fs::create_dir(fixture.root.join(directory)).unwrap();
        }
        // dbus-update --systemd requires XDG_RUNTIME_DIR/systemd to exist.
        // Its private socket stays absent, so app-scope startup is disabled.
        for directory in PROFILE_DIRS {
            fs::write(fixture.root.join(directory).join("untouched"), directory).unwrap();
        }
        let helper = fixture.root.join("child.py");
        fs::write(&helper, include_str!("child.py")).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        for binary in [
            "hearthspace-settingsd",
            "hearthspace-compositor",
            "hearthspace-shell",
            "systemctl",
            "systemd-run",
        ] {
            symlink(&helper, fixture.root.join("bin").join(binary)).unwrap();
        }
        for index in 0..2 {
            let name = format!("org.hearthspace.FailedActivationProbe{index}");
            fs::write(
                fixture
                    .root
                    .join("services")
                    .join(format!("{name}.service")),
                format!(
                    "[D-BUS Service]\nName={name}\nExec=/usr/bin/python3 -I -S {} probe {}\n",
                    helper.display(),
                    fixture.root.join(format!("probe-{index}.json")).display()
                ),
            )
            .unwrap();
        }
        let config = fixture.root.join("bus.conf");
        // No host includes or service directories: only these two probes can
        // activate, and systemd is the in-process synthetic manager below.
        fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>{}</listen><auth>EXTERNAL</auth>\
             <servicedir>{}/services</servicedir><policy context=\"default\">\
             <allow user=\"*\"/><allow own=\"*\"/><allow send_destination=\"*\"/>\
             <allow receive_sender=\"*\"/></policy></busconfig>",
                fixture.address(),
                fixture.root.display()
            ),
        )
        .unwrap();
        let mut broker = Command::new("/usr/bin/dbus-daemon");
        broker
            .env_clear()
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .arg(format!("--config-file={}", config.display()));
        let broker = fixture.spawn(broker, "broker");
        loop {
            fixture.assert_broker_alive();
            if fixture
                .log(broker)
                .lines()
                .any(|line| line.starts_with(&fixture.address()))
            {
                break;
            }
            fixture.tick("broker readiness");
        }
        fixture.previous = BTreeMap::from([
            (
                "XDG_RUNTIME_DIR".into(),
                fixture.host.to_str().unwrap().into(),
            ),
            ("WAYLAND_DISPLAY".into(), "previous-wayland".into()),
            ("PATH".into(), "/usr/bin:/bin".into()),
        ]);
        fixture.state.lock().unwrap().values = fixture.previous.clone();
        fixture.manager = Some(fixture.run(async {
            zbus::connection::Builder::address(fixture.address().as_str())
                .unwrap()
                .method_timeout(Duration::from_secs(1))
                .serve_at("/org/freedesktop/systemd1", Manager(fixture.state.clone()))
                .unwrap()
                .name("org.freedesktop.systemd1")
                .unwrap()
                .build()
                .await
                .unwrap()
        }));
        fixture.run(async {
            let proxy = fixture.bus_proxy().await;
            let _: () = proxy
                .call("UpdateActivationEnvironment", &(&fixture.previous,))
                .await
                .unwrap();
        });
        fixture
    }

    pub fn address(&self) -> String {
        format!("unix:path={}/bus", self.host.display())
    }

    fn run<T>(&self, future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                tokio::time::timeout_at(
                    self.work_deadline
                        .min(Instant::now() + Duration::from_secs(2))
                        .into(),
                    future,
                )
                .await
                .expect("isolated bus operation exceeded fixture deadline")
            })
    }

    async fn bus_proxy(&self) -> zbus::Proxy<'_> {
        zbus::Proxy::new(
            self.manager.as_ref().unwrap(),
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await
        .unwrap()
    }

    fn command(&self, binary: &str) -> Command {
        let mut command = Command::new(binary);
        command
            .env_clear()
            .current_dir(&self.root)
            .env("HEARTHSPACE_ACTIVATION_TEST_ROOT", &self.root)
            .env("XDG_RUNTIME_DIR", &self.host)
            .env("HEARTHSPACE_HOST_XDG_RUNTIME_DIR", &self.host)
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}/unused-bus", self.root.display()),
            )
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}/no-system-bus", self.root.display()),
            )
            .env("HOME", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("XDG_CONFIG_DIRS", self.root.join("config"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_DATA_DIRS", self.root.join("data"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_STATE_HOME", self.root.join("state"))
            .env("TMPDIR", &self.root)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("bin").display()),
            );
        command
    }

    pub fn start_supervisor(&mut self) -> usize {
        assert!(!self.host.join("systemd/private").exists());
        let mut command = self.command(env!("CARGO_BIN_EXE_hearthspace-session"));
        command
            .args([
                "--installed",
                "--backend",
                "native",
                "--no-desktop-services",
                "--no-watch-apps",
                "--no-settingsd-restart",
                "--no-shell-restart",
                "--startup-timeout",
                "4",
                "--wayland-display",
                "wayland-failure",
            ])
            .arg("--binary-dir")
            .arg(self.root.join("bin"))
            .arg("--session-dir")
            .arg(&self.runtime);
        self.spawn(command, "supervisor")
    }

    pub fn start_retirement(&mut self) -> usize {
        let mut command = self.command(env!("CARGO_BIN_EXE_hearthspace-app-scope-guard"));
        command.arg("--retire-activation").arg(&self.runtime);
        self.spawn(command, "retirement")
    }

    fn spawn(&mut self, mut command: Command, name: &str) -> usize {
        let output = self.root.join(format!("{name}.log"));
        let log = fs::File::create(&output).unwrap();
        let child = command
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        self.children.push((child, output));
        self.children.len() - 1
    }

    pub fn wait(&mut self, index: usize) -> ExitStatus {
        loop {
            if let Some(status) = self.children[index].0.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < self.work_deadline,
                "child timed out: {}",
                self.log(index)
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn log(&self, index: usize) -> String {
        fs::read_to_string(&self.children[index].1).unwrap_or_default()
    }

    fn tick(&self, operation: &str) {
        assert!(
            Instant::now() < self.work_deadline,
            "fixture timed out during {operation}"
        );
        thread::sleep(Duration::from_millis(10));
    }

    pub fn assert_broker_alive(&mut self) {
        assert!(
            self.children[0].0.try_wait().unwrap().is_none(),
            "borrowed broker exited: {}",
            self.log(0)
        );
    }

    pub fn child_identity(&self, role: &str) -> serde_json::Value {
        serde_json::from_slice(&fs::read(self.root.join(format!("{role}-started.json"))).unwrap())
            .unwrap()
    }

    pub fn assert_children_stopped(&self) {
        for role in ["settingsd", "compositor"] {
            let identity = self.child_identity(role);
            let stopped: serde_json::Value = serde_json::from_slice(
                &fs::read(self.root.join(format!("{role}-stopped.json"))).unwrap_or_else(|error| {
                    panic!("{role} did not receive supervisor SIGTERM: {error}")
                }),
            )
            .unwrap();
            assert_eq!(stopped, identity);
            assert!(!same_process(&identity), "supervisor left {role} unreaped");
        }
        assert!(
            !self.root.join("unexpected-helper").exists(),
            "shell or manager helper unexpectedly launched"
        );
    }

    pub fn assert_profile_untouched(&self) {
        for directory in PROFILE_DIRS {
            let directory_path = self.root.join(directory);
            assert_eq!(
                fs::read_to_string(directory_path.join("untouched")).unwrap(),
                *directory
            );
            assert_eq!(fs::read_dir(directory_path).unwrap().count(), 1);
        }
    }

    pub fn probe(&self, index: usize) -> BTreeMap<String, Option<String>> {
        assert!(index < 2);
        self.run(async {
            let result: zbus::Result<u32> = self
                .bus_proxy()
                .await
                .call(
                    "StartServiceByName",
                    &(
                        format!("org.hearthspace.FailedActivationProbe{index}"),
                        0_u32,
                    ),
                )
                .await;
            assert!(result.is_err(), "probe unexpectedly acquired a name");
        });
        serde_json::from_slice(
            &fs::read(self.root.join(format!("probe-{index}.json")))
                .expect("fresh D-Bus activation must write its probe"),
        )
        .unwrap()
    }
}

fn same_process(identity: &serde_json::Value) -> bool {
    let stat = fs::read_to_string(format!("/proc/{}/stat", identity["pid"].as_u64().unwrap()));
    // Only inspect the two owned stubs' stat records, never process environments.
    stat.ok()
        .and_then(|stat| {
            stat.rsplit_once(')')
                .and_then(|(_, fields)| fields.split_whitespace().nth(19).map(str::to_owned))
        })
        .as_deref()
        == identity["start_time"].as_str()
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.manager.take();
        // Kill only still-owned Child handles. Stub PDEATHSIG covers each
        // separately-sessioned grandchild if startup assertions fail. All
        // teardown shares the original deadline; no unbounded wait(), joins,
        // or per-child timeout extensions, including while unwinding.
        for (child, _) in &mut self.children {
            let _ = child.kill();
        }
        while Instant::now() < self.cleanup_deadline {
            let pending = self
                .children
                .iter_mut()
                .any(|(child, _)| child.try_wait().is_ok_and(|status| status.is_none()));
            if !pending {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
