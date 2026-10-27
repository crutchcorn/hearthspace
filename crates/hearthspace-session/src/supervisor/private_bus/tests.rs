use std::{
    env,
    os::unix::fs::{DirBuilderExt, symlink},
    path::Path,
    sync::atomic::AtomicU64,
};

use clap::Parser;
use serde_json::json;

use super::*;

struct Fixture {
    root: PathBuf,
    address: String,
    supervisor: Supervisor,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = env::temp_dir().join(format!(
            "hs-owned-bus-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let runtime = root.join("runtime");
        fs::DirBuilder::new().mode(0o700).create(&runtime).unwrap();
        let address = crate::private_bus_address::generate().unwrap();
        let args = Args::parse_from([
            "session",
            "--installed",
            "--backend",
            "native",
            "--private-session-bus",
        ]);
        let supervisor = Supervisor::new(
            RunMode::InstalledBinaries,
            SessionPaths {
                host_runtime_dir: root.clone(),
                session_dir: runtime.clone(),
                applications_dir: runtime.join("share/applications"),
                settings_socket: runtime.join("settings.sock"),
                session_socket: runtime.join("session.sock"),
                shell_command_socket: runtime.join("shell.sock"),
            },
            SessionEnv {
                wayland_display: "wayland-fixture".into(),
                vars: vec![
                    ("XDG_RUNTIME_DIR".into(), runtime.display().to_string()),
                    (
                        "HEARTHSPACE_HOST_XDG_RUNTIME_DIR".into(),
                        root.display().to_string(),
                    ),
                    ("HEARTHSPACE_SESSION_BUS_ADDRESS".into(), address.clone()),
                    ("DBUS_SESSION_BUS_ADDRESS".into(), address.clone()),
                    ("XDG_DATA_DIRS".into(), root.display().to_string()),
                ],
            },
            Arc::new(AtomicBool::new(false)),
            &args,
            None,
        );
        Self {
            root,
            address,
            supervisor,
        }
    }

    fn start(&mut self) {
        self.supervisor
            .start_session_bus(Some(&self.address))
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.supervisor.shutdown();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn connection(address: &str) -> zbus::Connection {
    hearthspace_desktop::activation::operations::bounded(
        Instant::now() + Duration::from_secs(2),
        &AtomicBool::new(false),
        async {
            zbus::connection::Builder::address(address)
                .map_err(io::Error::other)?
                .build()
                .await
                .map_err(io::Error::other)
        },
    )
    .unwrap()
}

#[test]
fn ready_broker_is_recorded_and_stopped_without_restarting() {
    let mut fixture = Fixture::new();
    fixture.start();
    let broker = fixture.supervisor.session_bus.as_ref().unwrap();
    let pid = broker.pid();
    assert_eq!(broker.max_restarts, 0);
    let record: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.supervisor.paths.session_dir.join("processes.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record["session_bus"], pid);
    let _connection = connection(&fixture.address);
    fixture.supervisor.shutdown().unwrap();
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(!fixture.supervisor.paths.session_dir.exists());
}

#[test]
fn cleanup_barrier_failure_reaps_broker_but_retains_recovery_records() {
    let mut fixture = Fixture::new();
    fixture.start();
    let pid = fixture.supervisor.session_bus.as_ref().unwrap().pid();
    let intent = fixture.supervisor.paths.session_dir.join("app-scope-owner");
    fs::write(&intent, "invalid-owner").unwrap();
    let error = fixture.supervisor.shutdown().unwrap_err();
    assert!(error.to_string().contains("owner intent does not match"));
    assert_eq!(fs::read_to_string(intent).unwrap(), "invalid-owner");
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(
        fixture
            .supervisor
            .paths
            .session_dir
            .join("processes.json")
            .exists()
    );
}

#[test]
fn a_listening_socket_without_a_bus_reply_cannot_pass_readiness() {
    use std::os::unix::net::UnixListener;
    let mut fixture = Fixture::new();
    let socket = fixture.root.join("unresponsive.sock");
    let _listener = UnixListener::bind(&socket).unwrap();
    fixture.supervisor.startup_deadline = Instant::now() + Duration::from_millis(120);
    let started = Instant::now();
    let error = fixture
        .supervisor
        .wait_for_session_bus(&format!("unix:path={}", socket.display()))
        .unwrap_err();
    assert!(error.to_string().contains("absolute startup deadline"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
#[ignore = "requires an explicitly prebuilt HEARTHSPACE_TEST_NOTIFICATIONS_BIN; see test comment"]
fn actual_notifications_bind_privately_and_leave_the_existing_provider_alive() {
    // cargo build --locked -p hearthspace-notifications --bin hearthspace-notifications
    // HEARTHSPACE_TEST_NOTIFICATIONS_BIN=/absolute/artifact/path cargo test --locked
    //   -p hearthspace-session actual_notifications_bind_privately -- --ignored --test-threads=1
    // The caller prebuilds this package explicitly. Never guess a possibly stale
    // target/debug sibling or invoke nested Cargo from an integration test.
    let executable = PathBuf::from(env::var_os("HEARTHSPACE_TEST_NOTIFICATIONS_BIN")
        .expect("prebuild hearthspace-notifications and set HEARTHSPACE_TEST_NOTIFICATIONS_BIN to its absolute executable path"));
    assert!(executable.is_absolute() && executable.is_file());
    let mut host = Fixture::new();
    host.start();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let provider = runtime.block_on(async {
        zbus::connection::Builder::address(host.address.as_str())
            .unwrap()
            .allow_name_replacements(false)
            .replace_existing_names(false)
            .name("org.freedesktop.Notifications")
            .unwrap()
            .build()
            .await
            .unwrap()
    });
    let owner = provider.unique_name().unwrap().to_owned();
    let mut desktop = Fixture::new();
    symlink(&executable, desktop.root.join("hearthspace-notifications")).unwrap();
    desktop.supervisor.binary_dir = Some(desktop.root.clone());
    desktop.start();
    desktop.supervisor.start_notifications().unwrap();
    let pid = desktop.supervisor.notifications.as_ref().unwrap().pid();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(3), async {
            let client = zbus::connection::Builder::address(desktop.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap();
            let proxy = zbus::Proxy::new(
                &client,
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
            )
            .await
            .unwrap();
            let server: (String, String, String, String) =
                proxy.call("GetServerInformation", &()).await.unwrap();
            assert_eq!(server.0, "Hearthspace");
        })
        .await
        .unwrap();
    });
    let response = hearthspace_ipc::transport::request_line(
        &desktop
            .supervisor
            .paths
            .session_dir
            .join("hearthspace-notifications.sock"),
        &format!("{}\n", json!({"command":"list"})),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&response).unwrap()["ok"],
        true
    );
    desktop.supervisor.shutdown().unwrap();
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(2), async {
            let fresh = zbus::connection::Builder::address(host.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap();
            let bus = zbus::fdo::DBusProxy::new(&fresh).await.unwrap();
            assert_eq!(
                bus.get_name_owner("org.freedesktop.Notifications".try_into().unwrap())
                    .await
                    .unwrap(),
                owner
            );
        })
        .await
        .unwrap();
    });
    assert!(host.supervisor.check_session_bus().is_ok());
}
