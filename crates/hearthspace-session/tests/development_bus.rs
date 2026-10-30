//! Real supervisor and brokers; Cargo/display children are disposable fixtures.
//! This proves bus policy/lifecycle, not native presentation or input behavior.
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use serde_json::{Value, json};
use zbus::{Connection, connection::Builder, fdo::DBusProxy};

#[path = "development_bus/support.rs"]
mod support;
use support::{Fixture, assert_dead};

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
struct Notifications;

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("Disposable fixture", "Hearthspace tests", "1", "1.3")
    }
}

struct HostManager(Arc<AtomicUsize>);
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl HostManager {
    #[zbus(property)]
    fn environment(&self) -> Vec<String> {
        self.0.fetch_add(1, Ordering::Relaxed);
        vec!["WAYLAND_DISPLAY=fixture-host".into()]
    }
    fn set_environment(&self, _values: Vec<String>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn unset_and_set_environment(&self, _unset: Vec<String>, _set: Vec<String>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

async fn connect(address: &str) -> Connection {
    tokio::time::timeout(Duration::from_secs(2), async {
        Builder::address(address).unwrap().build().await.unwrap()
    })
    .await
    .unwrap()
}

async fn snapshot(connection: &Connection) -> (String, String) {
    tokio::time::timeout(Duration::from_secs(2), async {
        let proxy = DBusProxy::new(connection).await.unwrap();
        (
            proxy.get_id().await.unwrap().to_string(),
            proxy
                .get_name_owner(NOTIFICATIONS.try_into().unwrap())
                .await
                .unwrap()
                .to_string(),
        )
    })
    .await
    .unwrap()
}

async fn activation(connection: &Connection, output: &Path) -> Value {
    let proxy = zbus::Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await
    .unwrap();
    // The real activated child claims its name and records only synthetic keys.
    let result: zbus::Result<u32> = tokio::time::timeout(
        Duration::from_secs(3),
        proxy.call(
            "StartServiceByName",
            &("org.hearthspace.DevBusProbe", 0_u32),
        ),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap(), 1);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(bytes) = fs::read(output) {
            return serde_json::from_slice(&bytes).unwrap();
        }
        assert!(Instant::now() < deadline, "activation probe did not run");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
fn development_activation_probe() {
    if env::var_os("HEARTHSPACE_TEST_DEV_BUS_ACTIVATE").is_none() {
        return;
    }
    let root = PathBuf::from(env::var_os("HEARTHSPACE_TEST_DEV_BUS_ROOT").unwrap());
    let directory = PathBuf::from(env::var_os("XDG_RUNTIME_DIR").unwrap());
    assert!(directory == root.join("host") || directory == root.join("runtime"));
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async {
        let connection = tokio::time::timeout(Duration::from_secs(2), Connection::session())
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(2),
            connection.request_name("org.hearthspace.DevBusProbe"),
        )
        .await
        .unwrap()
        .unwrap();
        let values: std::collections::BTreeMap<_, _> = [
            "XDG_RUNTIME_DIR",
            "WAYLAND_DISPLAY",
            "DISPLAY",
            "WAYLAND_SOCKET",
            "AT_SPI_BUS_ADDRESS",
            "XDG_CURRENT_DESKTOP",
        ]
        .into_iter()
        .map(|key| (key, env::var(key).ok()))
        .collect();
        let output = root.join(format!(
            "{}-activation.json",
            directory.file_name().unwrap().to_str().unwrap()
        ));
        let pending = output.with_extension("tmp");
        fs::write(&pending, serde_json::to_vec(&values).unwrap()).unwrap();
        fs::rename(pending, output).unwrap();
        // NameAcquired completes StartServiceByName before this child retires.
        tokio::time::sleep(Duration::from_millis(50)).await;
    });
}

#[test]
fn development_shell_probe() {
    if env::var_os("HEARTHSPACE_TEST_DEV_BUS_CHILD").is_none() {
        return;
    }
    let root = PathBuf::from(env::var_os("HEARTHSPACE_TEST_DEV_BUS_ROOT").unwrap());
    let address = env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    // Connect only after proving this address was selected by our disposable
    // supervisor. A normal test invocation never uses the developer's bus.
    assert_eq!(
        address,
        fs::read_to_string(root.join("runtime/session-bus.txt")).unwrap()
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let provider = tokio::time::timeout(Duration::from_secs(3), async {
            Builder::session()
                .unwrap()
                .allow_name_replacements(false)
                .replace_existing_names(false)
                .serve_at("/org/freedesktop/Notifications", Notifications)
                .unwrap()
                .name(NOTIFICATIONS)
                .unwrap()
                .build()
                .await
                .unwrap()
        })
        .await
        .unwrap();
        let (bus_id, owner) = snapshot(&provider).await;
        let report = json!({
            "bus": address, "bus_id": bus_id, "owner": owner,
            "runtime": env::var("XDG_RUNTIME_DIR").unwrap(),
            "host_runtime": env::var("HEARTHSPACE_HOST_XDG_RUNTIME_DIR").unwrap(),
        });
        fs::write(root.join("shell.tmp"), serde_json::to_vec(&report).unwrap()).unwrap();
        fs::rename(root.join("shell.tmp"), root.join("shell.json")).unwrap();
        tokio::time::sleep(Duration::from_secs(25)).await;
    });
    panic!("fixture supervisor did not stop its shell");
}

#[tokio::test(flavor = "current_thread")]
async fn native_development_isolates_notifications_and_activation_from_existing_desktop() {
    let mut fixture = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let host = Builder::address(fixture.host_address.as_str())
        .unwrap()
        .allow_name_replacements(false)
        .replace_existing_names(false)
        .serve_at("/org/freedesktop/Notifications", Notifications)
        .unwrap()
        .serve_at("/org/freedesktop/systemd1", HostManager(calls.clone()))
        .unwrap()
        .name(NOTIFICATIONS)
        .unwrap()
        .name("org.freedesktop.systemd1")
        .unwrap()
        .build()
        .await
        .unwrap();
    let before = snapshot(&host).await;
    let host_activation = activation(&host, &fixture.root.join("host-activation.json")).await;
    fixture.start("ready", &[]);
    fixture.wait_ready();
    let record = fixture.record();
    let broker = record["session_bus"].as_u64().unwrap() as u32;
    assert_ne!(broker, 0);
    let shell: Value =
        serde_json::from_slice(&fs::read(fixture.root.join("shell.json")).unwrap()).unwrap();
    let address = shell["bus"].as_str().unwrap();
    assert!(address.starts_with("unix:abstract=/tmp/dbus-hearthspace-"));
    assert_ne!(address, fixture.host_address);
    assert_ne!(shell["bus_id"], before.0);
    assert_eq!(shell["runtime"], fixture.runtime.to_str().unwrap());
    assert_eq!(
        shell["host_runtime"],
        fixture.root.join("host").to_str().unwrap()
    );
    let private = connect(address).await;
    let actual = snapshot(&private).await;
    assert_eq!(actual.0, shell["bus_id"].as_str().unwrap());
    assert_eq!(actual.1, shell["owner"].as_str().unwrap());
    let activated = activation(&private, &fixture.root.join("runtime-activation.json")).await;
    assert_eq!(
        activated["XDG_RUNTIME_DIR"],
        fixture.runtime.to_str().unwrap()
    );
    assert_eq!(activated["XDG_CURRENT_DESKTOP"], "Hearthspace");
    assert_eq!(activated["WAYLAND_SOCKET"], Value::Null);
    assert_eq!(activated["AT_SPI_BUS_ADDRESS"], Value::Null);
    assert_eq!(activated["DISPLAY"], "");
    fixture.signal(Signal::SIGTERM);
    assert!(fixture.wait_exit().success());
    assert!(!fixture.runtime.exists());
    for key in ["session_bus", "settingsd", "compositor", "shell"] {
        assert_dead(record[key].as_u64().unwrap() as u32);
    }
    let fresh = connect(&fixture.host_address).await;
    assert_eq!(snapshot(&fresh).await, before);
    fs::remove_file(fixture.root.join("host-activation.json")).unwrap();
    assert_eq!(
        activation(&fresh, &fixture.root.join("host-activation.json")).await,
        host_activation
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    fixture.assert_host_alive();
}

#[test]
fn cancelled_startup_reaps_owned_bus_and_retains_host_bus() {
    let mut fixture = Fixture::new();
    fixture.start("stalled-settings", &[]);
    fixture.wait_file(&fixture.root.join("hearthspace-settingsd.started"));
    let record = fixture.record();
    fixture.signal(Signal::SIGTERM);
    assert!(!fixture.wait_exit().success());
    assert!(!fixture.runtime.exists());
    assert_dead(record["session_bus"].as_u64().unwrap() as u32);
    fixture.assert_host_alive();
}

#[test]
fn unexpected_broker_exit_ends_session_without_restarting_bus() {
    let mut fixture = Fixture::new();
    fixture.start("ready", &[]);
    fixture.wait_ready();
    let record = fixture.record();
    let broker = record["session_bus"].as_u64().unwrap() as u32;
    kill(Pid::from_raw(broker as i32), Signal::SIGTERM).unwrap();
    assert!(!fixture.wait_exit().success());
    let log = fs::read_to_string(fixture.root.join("supervisor.log")).unwrap();
    assert!(log.contains("private session bus exited"), "{log}");
    assert!(!log.contains("restarting process"), "{log}");
    assert!(!fixture.runtime.exists());
    assert_dead(broker);
    fixture.assert_host_alive();
}

#[test]
fn supervisor_death_cannot_orphan_private_broker() {
    let mut fixture = Fixture::new();
    fixture.start("stalled-settings", &[]);
    fixture.wait_file(&fixture.root.join("hearthspace-settingsd.started"));
    let record = fixture.record();
    fixture.signal(Signal::SIGKILL);
    assert!(!fixture.wait_exit().success());
    assert_dead(record["session_bus"].as_u64().unwrap() as u32);
    assert_dead(record["settingsd"].as_u64().unwrap() as u32);
    fixture.assert_host_alive();
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_private_development_keeps_supplied_broker_unowned() {
    let mut fixture = Fixture::new();
    fixture.start("ready", &["--private-session-bus"]);
    fixture.wait_ready();
    assert_eq!(fixture.record()["session_bus"], 0);
    let shell: Value =
        serde_json::from_slice(&fs::read(fixture.root.join("shell.json")).unwrap()).unwrap();
    assert_eq!(shell["bus"], fixture.host_address);
    fixture.signal(Signal::SIGTERM);
    assert!(fixture.wait_exit().success());
    let fresh = connect(&fixture.host_address).await;
    assert_eq!(
        DBusProxy::new(&fresh)
            .await
            .unwrap()
            .get_id()
            .await
            .unwrap()
            .to_string(),
        shell["bus_id"].as_str().unwrap()
    );
    fixture.assert_host_alive();
}

#[test]
fn automatic_private_dry_run_does_not_spawn_a_broker_or_children() {
    let mut fixture = Fixture::new();
    fixture.start("desktop-dry-run", &["--dry-run"]);
    assert!(fixture.wait_exit().success());
    let address = fs::read_to_string(fixture.runtime.join("session-bus.txt")).unwrap();
    assert!(address.starts_with("unix:abstract=/tmp/dbus-hearthspace-"));
    assert!(!fixture.runtime.join("processes.json").exists());
    assert!(!fixture.root.join("hearthspace-settingsd.started").exists());
    let mask = fs::read_to_string(
        fixture
            .runtime
            .join("dbus-1/services/org.freedesktop.portal.Desktop.service"),
    )
    .unwrap();
    assert!(mask.contains("Exec=/usr/bin/false"));
    assert!(!fixture.root.join("host/dbus-1").exists());
    let log = fs::read_to_string(fixture.root.join("supervisor.log")).unwrap();
    assert!(!log.contains("starting process"));
    fixture.assert_host_alive();
}
