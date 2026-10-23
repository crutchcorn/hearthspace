//! Real supervisor, disposable buses, and a synthetic unlocked collection.
//! No GPU/VT, host bus/keyring, activation publication, or desktop services.
use std::{
    collections::HashSet,
    env, fs,
    future::Future,
    io::{Read, Write},
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use zbus::{Connection, connection::Builder, fdo::DBusProxy};

#[path = "user_bus/support.rs"]
mod support;
use support::{Broker, Fixture, ROLE_ENV, ROOT_ENV};

const SECRET_SERVICE: &str = "org.freedesktop.secrets";
const COLLECTION: &str = "/org/freedesktop/secrets/collection/fixture";

struct Collection {
    locked: bool,
}

#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl Collection {
    #[zbus(property)]
    fn locked(&self) -> bool {
        self.locked
    }
}

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), future)
        .await
        .expect("fixture D-Bus operation timed out")
}

async fn provider(address: &str, locked: bool) -> Connection {
    bounded(async {
        Builder::address(address)
            .unwrap()
            .serve_at(COLLECTION, Collection { locked })
            .unwrap()
            .name(SECRET_SERVICE)
            .unwrap()
            .build()
            .await
            .unwrap()
    })
    .await
}

#[derive(Debug, Serialize, Deserialize)]
struct Observation {
    bus_id: String,
    owner: String,
    client: String,
    locked: bool,
}

async fn observe(connection: Connection) -> Observation {
    let bus = DBusProxy::new(&connection).await.unwrap();
    let collection = zbus::Proxy::new(
        &connection,
        SECRET_SERVICE,
        COLLECTION,
        "org.freedesktop.Secret.Collection",
    )
    .await
    .unwrap();
    Observation {
        bus_id: bus.get_id().await.unwrap().to_string(),
        owner: bus
            .get_name_owner(SECRET_SERVICE.try_into().unwrap())
            .await
            .unwrap()
            .to_string(),
        client: connection.unique_name().unwrap().to_string(),
        locked: collection.get_property("Locked").await.unwrap(),
    }
}

async fn fresh_client(address: &str) -> Observation {
    bounded(async { observe(Builder::address(address).unwrap().build().await.unwrap()).await })
        .await
}

#[derive(Debug, Serialize, Deserialize)]
struct ChildReport {
    address: String,
    session_address: String,
    host_runtime: PathBuf,
    runtime: PathBuf,
    pid: u32,
    start_time: String,
    clients: Vec<Observation>,
}

fn process_start_time(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    Some(
        stat.rsplit_once(") ")?
            .1
            .split_whitespace()
            .nth(19)?
            .to_owned(),
    )
}

// Only the supervisor's script fixtures set ROLE_ENV. Running the normal test
// suite cannot accidentally connect to the developer's session bus.
#[tokio::test(flavor = "current_thread")]
async fn user_bus_child_probe() {
    let Ok(role) = env::var(ROLE_ENV) else { return };
    assert!(matches!(role.as_str(), "settingsd" | "compositor"));
    let root = PathBuf::from(env::var_os(ROOT_ENV).unwrap());
    let address = env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    let canonical = env::var("HEARTHSPACE_TEST_USER_BUS_CANONICAL").unwrap();
    let supplied = env::var("HEARTHSPACE_TEST_USER_BUS_SUPPLIED").unwrap();
    assert!(
        address == canonical || address == supplied,
        "refusing connection to any bus not owned by this fixture: {address}"
    );
    let mut clients = Vec::new();
    for _ in 0..2 {
        // Each session() opens a new transport from the real child's environment.
        clients.push(bounded(async { observe(Connection::session().await.unwrap()).await }).await);
    }
    let report = ChildReport {
        address,
        session_address: env::var("HEARTHSPACE_SESSION_BUS_ADDRESS").unwrap(),
        host_runtime: env::var_os("HEARTHSPACE_HOST_XDG_RUNTIME_DIR")
            .unwrap()
            .into(),
        runtime: env::var_os("XDG_RUNTIME_DIR").unwrap().into(),
        pid: std::process::id(),
        start_time: process_start_time(std::process::id()).unwrap(),
        clients,
    };
    fs::write(
        root.join(format!("{role}.tmp")),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    fs::rename(
        root.join(format!("{role}.tmp")),
        root.join(format!("{role}.json")),
    )
    .unwrap();
    if role == "settingsd" {
        // Parent-death SIGKILL is only a leak guard. An acknowledgment of the
        // supervisor's graceful signal proves its own shutdown path ran.
        let stopping = Arc::new(AtomicBool::new(false));
        let signal_seen = Arc::clone(&stopping);
        ctrlc::set_handler(move || signal_seen.store(true, Ordering::SeqCst)).unwrap();
        let listener =
            UnixListener::bind(env::var_os("HEARTHSPACE_SETTINGS_SOCKET").unwrap()).unwrap();
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if stopping.load(Ordering::SeqCst) {
                fs::write(root.join("settingsd-stopped.tmp"), report.pid.to_string()).unwrap();
                fs::rename(
                    root.join("settingsd-stopped.tmp"),
                    root.join("settingsd-stopped"),
                )
                .unwrap();
                return;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_millis(250)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_millis(250)))
                        .unwrap();
                    let mut request = [0; 32];
                    if stream.read(&mut request).is_ok() {
                        let _ = stream.write_all(b"ok {}\n");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("settings fixture accept: {error}"),
            }
        }
        panic!("supervisor left the settings fixture running");
    }
    // The compositor deliberately exits without opening a Wayland socket.
    // Activation, shell startup, and user-manager updates must never be reached.
}

async fn assert_session(
    fixture: &Fixture,
    supplied: &str,
    selected: &str,
    expected: &Observation,
    private: bool,
) {
    fixture.run(supplied, private).await;
    let mut clients = HashSet::new();
    for role in ["settingsd", "compositor"] {
        let path = fixture.root.join(format!("{role}.json"));
        let report: ChildReport =
            serde_json::from_slice(&fs::read(&path).unwrap_or_else(|error| {
                panic!(
                    "{role} never completed its probe: {error}\n{}",
                    fs::read_to_string(fixture.root.join("supervisor.log")).unwrap_or_default()
                )
            }))
            .unwrap();
        assert_eq!(report.address, selected);
        assert_eq!(report.session_address, selected);
        assert_eq!(report.host_runtime, fixture.host);
        assert_eq!(report.runtime, fixture.runtime);
        assert_ne!(report.runtime, report.host_runtime);
        if role == "settingsd" {
            assert_eq!(
                fs::read_to_string(fixture.root.join("settingsd-stopped")).unwrap_or_default(),
                report.pid.to_string(),
                "supervisor did not gracefully terminate settingsd"
            );
        }
        assert_ne!(
            process_start_time(report.pid),
            Some(report.start_time),
            "startup cleanup left {role} running"
        );
        assert_eq!(report.clients.len(), 2);
        for observation in report.clients {
            assert_eq!(observation.bus_id, expected.bus_id, "{role}");
            assert_eq!(observation.owner, expected.owner, "{role}");
            assert!(!observation.locked, "{role} reached a locked collection");
            assert!(
                clients.insert(observation.client),
                "client connection was reused"
            );
        }
    }
    // Fresh clients after cleanup must still find the SAME provider and bus.
    // A restarted daemon/provider or a disconnected cached proxy cannot pass.
    for _ in 0..2 {
        let after = fresh_client(selected).await;
        assert_eq!(after.bus_id, expected.bus_id);
        assert_eq!(after.owner, expected.owner);
        assert!(!after.locked);
        assert!(clients.insert(after.client));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn native_children_reuse_unlocked_user_bus_after_failed_startup() {
    let fixture = Fixture::new();
    let mut canonical = Broker::start(&fixture.host);
    let mut other_desktop = Broker::start(&fixture.root.join("supplied"));
    let unlocked_provider = provider(&canonical.address, false).await;
    let _other_provider = provider(&other_desktop.address, true).await;
    let expected = fresh_client(&canonical.address).await;
    let other = fresh_client(&other_desktop.address).await;
    assert_ne!(expected.bus_id, other.bus_id);
    assert!(other.locked);
    assert_eq!(
        expected.owner,
        unlocked_provider.unique_name().unwrap().as_str()
    );
    let selected = fixture.canonical_address();
    assert!(selected.ends_with("host-runtime/bus"));
    assert_session(
        &fixture,
        &other_desktop.address,
        &selected,
        &expected,
        false,
    )
    .await;
    canonical.assert_running();
    other_desktop.assert_running();
    let after = fresh_client(&other_desktop.address).await;
    assert_eq!(after.bus_id, other.bus_id);
    assert_eq!(after.owner, other.owner);
    assert!(after.locked);
}

#[tokio::test(flavor = "current_thread")]
async fn private_children_preserve_supplied_bus_with_canonical_bus_present() {
    let fixture = Fixture::new();
    let mut canonical = Broker::start(&fixture.host);
    let mut private = Broker::start(&fixture.root.join("supplied"));
    let _host_provider = provider(&canonical.address, true).await;
    let unlocked_provider = provider(&private.address, false).await;
    let host = fresh_client(&canonical.address).await;
    let expected = fresh_client(&private.address).await;
    assert_ne!(expected.bus_id, host.bus_id);
    assert!(host.locked);
    assert_eq!(
        expected.owner,
        unlocked_provider.unique_name().unwrap().as_str()
    );
    // Preserve even the supplied daemon's GUID suffix, not just its socket.
    assert!(private.address.contains(",guid="));
    assert_session(
        &fixture,
        &private.address,
        &private.address,
        &expected,
        true,
    )
    .await;
    canonical.assert_running();
    private.assert_running();
    let after = fresh_client(&canonical.address).await;
    assert_eq!(after.bus_id, host.bus_id);
    assert_eq!(after.owner, host.owner);
    assert!(after.locked);
}
