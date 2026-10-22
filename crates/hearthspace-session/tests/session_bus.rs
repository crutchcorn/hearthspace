//! Exercise the actual daemon wrapper without a desktop, user manager or GPU.
use std::{
    env, fs,
    os::{
        linux::net::SocketAddrExt,
        unix::{
            fs::DirBuilderExt,
            net::{SocketAddr, UnixStream},
            process::CommandExt,
        },
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const PROBE_DIRECTORY: &str = "HEARTHSPACE_TEST_BUS_PROBE";

struct Bus {
    directory: PathBuf,
    child: Child,
    reaped: bool,
    address: String,
    id: String,
}

impl Bus {
    fn start(name: &str, helper: Option<&str>, inherited_address: &str) -> Self {
        let directory =
            env::temp_dir().join(format!("hearthspace-bus-{}-{name}", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let log = fs::File::create(directory.join("probe.log")).unwrap();
        let mut command = Command::new("/usr/bin/dbus-run-session");
        if let Some(helper) = helper {
            command.arg("--dbus-daemon").arg(helper);
        }
        let child = command
            .arg("--")
            .arg(env::current_exe().unwrap())
            .args(["--exact", "bus_probe_child", "--nocapture"])
            .env(PROBE_DIRECTORY, &directory)
            .env("XDG_RUNTIME_DIR", &directory)
            .env("DBUS_SESSION_BUS_ADDRESS", inherited_address)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .process_group(0)
            .spawn()
            .unwrap();
        let mut bus = Self {
            directory,
            child,
            reaped: false,
            address: String::new(),
            id: String::new(),
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !bus.directory.join("ready").is_file() {
            assert!(bus.poll().is_none(), "bus probe failed: {}", bus.log());
            assert!(
                Instant::now() < deadline,
                "bus readiness timed out: {}",
                bus.log()
            );
            thread::sleep(Duration::from_millis(10));
        }
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(bus.directory.join("ready")).unwrap()).unwrap();
        bus.address = report["address"].as_str().unwrap().into();
        bus.id = report["id"].as_str().unwrap().into();
        bus
    }

    fn log(&self) -> String {
        fs::read_to_string(self.directory.join("probe.log")).unwrap_or_default()
    }

    fn poll(&mut self) -> Option<std::process::ExitStatus> {
        let status = self.child.try_wait().unwrap();
        self.reaped |= status.is_some();
        status
    }

    fn finish(&mut self) {
        fs::write(self.directory.join("exit"), "").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.poll() {
                assert!(status.success(), "bus probe failed: {}", self.log());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "dbus-run-session did not finish: {}",
                self.log()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        // A reaped runner's PID/PGID may already belong to a different process.
        if !self.reaped {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(self.child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            let _ = self.child.wait();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn bus_probe_child() {
    let Some(directory) = env::var_os(PROBE_DIRECTORY).map(PathBuf::from) else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let id = runtime.block_on(async {
        // Parse exactly the environment supplied by dbus-run-session. This also
        // rejects a semicolon address list unsupported by current zbus clients.
        let connection = zbus::Connection::session().await.unwrap();
        zbus::fdo::DBusProxy::new(&connection)
            .await
            .unwrap()
            .get_id()
            .await
            .unwrap()
            .to_string()
    });
    let report =
        serde_json::json!({"address": env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(), "id": id});
    fs::write(directory.join("ready.tmp"), report.to_string()).unwrap();
    fs::rename(directory.join("ready.tmp"), directory.join("ready")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !directory.join("exit").exists() {
        assert!(
            Instant::now() < deadline,
            "probe parent did not release the fixture"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn private_bus_uses_one_abstract_address_and_ends_with_its_session() {
    let inherited = env::var_os("DBUS_SESSION_BUS_ADDRESS");
    // An independent bus stands in for the inherited host desktop; this test
    // never depends on or updates the real user's bus or activation environment.
    let mut host = Bus::start("host", None, "unix:path=/not-an-inherited-bus");
    let mut private = Bus::start(
        "private",
        Some(env!("CARGO_BIN_EXE_hearthspace-session-bus")),
        &host.address,
    );
    assert!(
        private
            .address
            .starts_with("unix:abstract=/tmp/dbus-hearthspace-"),
        "{}",
        private.address
    );
    assert!(!private.address.contains(';'));
    assert_ne!(private.address, host.address);
    assert_ne!(private.id, host.id);
    assert_eq!(env::var_os("DBUS_SESSION_BUS_ADDRESS"), inherited);
    let socket = private
        .address
        .strip_prefix("unix:abstract=")
        .unwrap()
        .split(',')
        .next()
        .unwrap();
    let socket = SocketAddr::from_abstract_name(socket.as_bytes()).unwrap();
    assert!(UnixStream::connect_addr(&socket).is_ok());
    private.finish();
    assert!(
        UnixStream::connect_addr(&socket).is_err(),
        "private bus survived its session"
    );
    assert!(
        host.poll().is_none(),
        "private shutdown affected the inherited bus"
    );
    host.finish();
}
