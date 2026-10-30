use super::*;
use std::{
    os::unix::fs::PermissionsExt,
    process::Child,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) const BORROWED: &[&str] = &[
    "org.freedesktop.impl.portal.PermissionStore",
    "org.freedesktop.portal.Documents",
    "org.freedesktop.impl.portal.desktop.gtk",
    "org.freedesktop.portal.Desktop",
];
pub(super) const DISTRO_BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.wlr";

pub(super) struct Fixture {
    broker: Child,
    pub root: PathBuf,
    pub paths: SessionPaths,
    pub session: SessionEnv,
    pub bus: String,
    pub backend: PathBuf,
    pub picker: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = env::temp_dir().join(format!(
            "hearthspace-cast-services-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let services = activation_directory(&root);
        let config = root.join("bus.conf");
        fs::write(&config, format!("<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><servicedir>{services}</servicedir><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>")).unwrap();
        let address = root.join("address");
        let mut broker = Command::new("dbus-daemon")
            .args(["--nofork", "--print-address=1", "--nopidfile"])
            .arg(format!("--config-file={}", config.display()))
            .stdout(fs::File::create(&address).unwrap())
            .spawn()
            .unwrap();
        let limit = deadline();
        let bus = loop {
            if let Ok(text) = fs::read_to_string(&address)
                && let Some(address) = text.strip_suffix('\n')
            {
                break address.to_owned();
            }
            assert!(broker.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < limit,
                "isolated broker did not become ready"
            );
            thread::sleep(Duration::from_millis(5));
        };
        let private = root.join("session");
        fs::create_dir(&private).unwrap();
        let paths = SessionPaths {
            host_runtime_dir: root.clone(),
            session_dir: private.clone(),
            applications_dir: private.join("share/applications"),
            settings_socket: private.join("settings.sock"),
            session_socket: private.join("session.sock"),
            shell_command_socket: private.join("commands.sock"),
        };
        let session = SessionEnv {
            wayland_display: "wayland-fixture".into(),
            vars: vec![
                ("XDG_RUNTIME_DIR".into(), private.to_str().unwrap().into()),
                (
                    "WAYLAND_DISPLAY".into(),
                    private.join("wayland-fixture").to_str().unwrap().into(),
                ),
                ("HEARTHSPACE_SESSION_BUS_ADDRESS".into(), bus.clone()),
                (
                    "PIPEWIRE_RUNTIME_DIR".into(),
                    root.join("pipewire").to_str().unwrap().into(),
                ),
                ("PIPEWIRE_REMOTE".into(), "fixture-pipewire".into()),
                ("PIPEWIRE_CORE".into(), "fixture-core".into()),
            ],
        };
        let backend = root.join("backend");
        script(&backend, include_str!("backend.py"));
        let picker = root.join("share ' picker $(invalid-command)");
        script(&picker, "#!/bin/sh\nprintf '%s\\n' \"$0\"\n");
        script(&root.join("systemctl"), include_str!("systemctl.py"));
        Self {
            broker,
            root,
            paths,
            session,
            bus,
            backend,
            picker,
        }
    }

    pub fn configure(&self, spec: &mut ProcessSpec, label: &str, names: &[&str]) {
        spec.envs.extend([
            (
                "HEARTHSPACE_TEST_CAST_ROOT".into(),
                self.root.to_str().unwrap().into(),
            ),
            (
                "HEARTHSPACE_TEST_CAST_EXE".into(),
                env::current_exe().unwrap().to_str().unwrap().into(),
            ),
            ("HEARTHSPACE_TEST_CAST_LABEL".into(), label.into()),
            ("HEARTHSPACE_TEST_CAST_NAMES".into(), names.join(",")),
        ]);
    }

    pub fn plan(&self, private_session_bus: bool) -> LaunchPlan {
        let mut capture =
            screencast::spec(&self.paths, &self.session, &self.backend, &self.picker).unwrap();
        self.configure(&mut capture, "capture", &[screencast::BUS_NAME]);
        let services = if private_session_bus {
            [("portal-gtk", BORROWED[2]), ("desktop-portal", BORROWED[3])]
                .into_iter()
                .map(|(label, name)| {
                    let mut spec = service_spec(
                        &self.paths,
                        &self.session,
                        label,
                        self.backend.to_str().unwrap(),
                    )
                    .unwrap();
                    self.configure(&mut spec, label, &[name]);
                    (spec, Some(name))
                })
                .collect()
        } else {
            Vec::new()
        };
        LaunchPlan {
            bus: self.bus.clone(),
            private_session_bus,
            capture,
            services,
            systemctl: self.root.join("systemctl"),
        }
    }

    pub fn foreign(&self, names: &[&str]) -> Foreign {
        let mut spec = service_spec(
            &self.paths,
            &self.session,
            "fixture-foreign",
            self.backend.to_str().unwrap(),
        )
        .unwrap();
        self.configure(&mut spec, "foreign", names);
        let mut owner = Foreign(Some(ManagedProcess::spawn(spec, 0).unwrap()));
        let limit = deadline();
        for name in names {
            while owner_pid(&self.bus, name, limit, &AtomicBool::new(false)).unwrap()
                != Some(owner.pid())
            {
                assert!(owner.running());
                assert!(
                    Instant::now() < limit,
                    "foreign fixture failed to own its name"
                );
                thread::sleep(Duration::from_millis(5));
            }
        }
        owner
    }

    pub fn snapshot(&self, label: &str) -> serde_json::Value {
        serde_json::from_slice(&fs::read(self.root.join(format!("{label}.json"))).unwrap()).unwrap()
    }

    pub fn calls(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(self.root.join("commands.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.broker.kill();
        let _ = self.broker.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(super) fn script(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn activation_directory(root: &Path) -> String {
    let services = root.join("services");
    fs::create_dir(&services).unwrap();
    let backend = root.join("distro-backend.py");
    script(&backend, include_str!("distro-backend.py"));
    let executable = backend
        .to_str()
        .unwrap()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    fs::write(
        services.join(format!("{DISTRO_BUS_NAME}.service")),
        format!("[D-BUS Service]\nName={DISTRO_BUS_NAME}\nExec=\"{executable}\"\n"),
    )
    .unwrap();
    services
        .to_str()
        .unwrap()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) struct Foreign(Option<ManagedProcess>);
impl Foreign {
    pub fn pid(&self) -> u32 {
        self.0.as_ref().unwrap().pid()
    }
    pub fn running(&mut self) -> bool {
        poll_process(&mut self.0).unwrap().is_none()
    }
}
impl Drop for Foreign {
    fn drop(&mut self) {
        terminate_process(&mut self.0);
        wait_then_kill_process_with_timeout(&mut self.0, Duration::from_millis(500));
    }
}

#[derive(Default)]
pub(super) struct Services(pub DesktopServices);
impl Drop for Services {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

struct BackendProbe;
#[zbus::interface(name = "org.hearthspace.TestCapture")]
impl BackendProbe {
    fn ready(&self) -> bool {
        true
    }
}

#[test]
#[ignore = "external service process for isolated ScreenCast lifecycle tests"]
fn portal_helper() {
    let Ok(root) = env::var("HEARTHSPACE_TEST_CAST_ROOT") else {
        return;
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let bus = env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
            let connection = zbus::connection::Builder::address(bus.as_str())
                .unwrap()
                .method_timeout(Duration::from_secs(2))
                .serve_at("/org/freedesktop/portal/desktop", BackendProbe)
                .unwrap()
                .build()
                .await
                .unwrap();
            let label = env::var("HEARTHSPACE_TEST_CAST_LABEL").unwrap();
            if !Path::new(&root).join(format!("stall-{label}")).exists() {
                for name in env::var("HEARTHSPACE_TEST_CAST_NAMES")
                    .unwrap()
                    .split(',')
                    .filter(|name| !name.is_empty())
                {
                    connection.request_name(name).await.unwrap();
                }
            }
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });
}
