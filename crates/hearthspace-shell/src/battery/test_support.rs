use std::{
    io::{BufRead, BufReader},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use zbus::{Connection, zvariant::OwnedValue};

use super::{
    BatteryStatus,
    transport::{Config, DEVICE, DISPLAY, ROOT, SERVICE},
};

pub struct Bus {
    child: Child,
    pub address: String,
    _files: Option<tempfile::TempDir>,
    _stdout: BufReader<ChildStdout>,
}

impl Bus {
    pub fn start() -> Self {
        Self::launch(None)
    }

    pub fn activatable() -> Self {
        let files = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap().display().to_string();
        let executable = executable.replace('\\', "\\\\").replace('"', "\\\"");
        std::fs::write(files.path().join(format!("{SERVICE}.service")), format!(
            "[D-BUS Service]\nName={SERVICE}\nExec=/usr/bin/env HEARTHSPACE_TEST_BATTERY_ACTIVATION=1 \"{executable}\" --exact battery::protocol_tests::activation_service_process --nocapture\n"
        )).unwrap();
        std::fs::write(files.path().join("bus.conf"), format!(
            "<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><servicedir>{}</servicedir><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",
            files.path().display()
        )).unwrap();
        Self::launch(Some(files))
    }

    fn launch(files: Option<tempfile::TempDir>) -> Self {
        let mut command = Command::new("dbus-daemon");
        match &files {
            Some(files) => {
                command
                    .arg("--config-file")
                    .arg(files.path().join("bus.conf"));
            }
            None => {
                command.arg("--session");
            }
        }
        let mut child = command
            .args(["--nofork", "--print-address=1", "--nopidfile"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon required for battery tests");
        let mut address = String::new();
        // Activated fixture processes inherit stdout, so keep the reader open
        // until bus teardown rather than giving them a broken pipe.
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        stdout.read_line(&mut address).unwrap();
        Self {
            child,
            address: address.trim().into(),
            _files: files,
            _stdout: stdout,
        }
    }

    pub async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .method_timeout(Duration::from_secs(1))
            .build()
            .await
            .unwrap()
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Monitor {
    pub updates: mpsc::UnboundedReceiver<Option<BatteryStatus>>,
    task: tokio::task::JoinHandle<()>,
}

impl Monitor {
    pub fn start(bus: &Bus, notification_bus: Option<&Bus>) -> Self {
        let (sender, updates) = mpsc::unbounded_channel();
        let task = tokio::spawn(super::transport::run(
            sender,
            notification_bus.map(|bus| bus.address.clone()),
            Config {
                system_address: Some(bus.address.clone()),
                // Long fallback proves the test's changes arrive via signals.
                poll: Duration::from_secs(30),
                retry: Duration::from_millis(40),
                timeout: Duration::from_millis(500),
            },
        ));
        Self { updates, task }
    }

    pub async fn next(
        &mut self,
        expected: impl Fn(Option<BatteryStatus>) -> bool,
    ) -> Option<BatteryStatus> {
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                let status = self.updates.recv().await.expect("monitor stopped");
                if expected(status) {
                    return status;
                }
            }
        })
        .await
        .expect("timed out waiting for battery update")
    }

    pub async fn stop(self) {
        drop(self.updates);
        tokio::time::timeout(Duration::from_secs(2), self.task)
            .await
            .unwrap()
            .unwrap();
    }
}

#[derive(Clone)]
pub struct State {
    pub stalled: bool,
    pub present: bool,
    pub kind: u32,
    pub percentage: f64,
    pub state: u32,
    pub warning: u32,
    pub on_battery: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            stalled: false,
            present: true,
            kind: 2,
            percentage: 72.0,
            state: 1,
            warning: 1,
            on_battery: false,
        }
    }
}

type Shared = Arc<Mutex<State>>;
struct Root(Shared);
#[zbus::interface(name = "org.freedesktop.UPower")]
impl Root {
    #[zbus(property)]
    fn on_battery(&self) -> bool {
        self.0.lock().unwrap().on_battery
    }
}

struct Device(Shared);
#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl Device {
    #[zbus(property)]
    fn is_present(&self) -> bool {
        self.0.lock().unwrap().present
    }
    #[zbus(property, name = "Type")]
    fn device_type(&self) -> u32 {
        self.0.lock().unwrap().kind
    }
    #[zbus(property)]
    async fn percentage(&self) -> f64 {
        let state = self.0.lock().unwrap().clone();
        if state.stalled {
            std::future::pending::<()>().await;
        }
        state.percentage
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0.lock().unwrap().state
    }
    #[zbus(property)]
    fn warning_level(&self) -> u32 {
        self.0.lock().unwrap().warning
    }
}

pub struct Power {
    pub connection: Connection,
    pub state: Shared,
}

impl Power {
    pub async fn start(bus: &Bus, state: State) -> Self {
        let connection = bus.connection().await;
        Self::on_connection(connection, state).await
    }

    pub async fn on_connection(connection: Connection, state: State) -> Self {
        let state = Arc::new(Mutex::new(state));
        connection
            .object_server()
            .at(ROOT, Root(state.clone()))
            .await
            .unwrap();
        connection
            .object_server()
            .at(DISPLAY, Device(state.clone()))
            .await
            .unwrap();
        connection.request_name(SERVICE).await.unwrap();
        Self { connection, state }
    }

    pub async fn change(&self, update: impl FnOnce(&mut State)) {
        update(&mut self.state.lock().unwrap());
        // Invalidation-only updates exercise the same fresh GetAll path as
        // changed-property values and require no cache population.
        self.connection
            .emit_signal(
                None::<&str>,
                DISPLAY,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(
                    DEVICE,
                    std::collections::HashMap::<&str, OwnedValue>::new(),
                    vec!["Percentage", "State", "WarningLevel", "IsPresent"],
                ),
            )
            .await
            .unwrap();
    }

    pub async fn stop(self) {
        self.connection.close().await.unwrap();
    }
}

#[derive(Clone, Debug)]
pub struct Notice {
    pub summary: String,
    pub urgency: u8,
}
pub struct Notifications(pub mpsc::UnboundedSender<Notice>);
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app_name: &str,
        _replaces_id: u32,
        _icon: &str,
        summary: &str,
        _body: &str,
        _actions: Vec<String>,
        hints: std::collections::HashMap<String, OwnedValue>,
        _expire: i32,
    ) -> u32 {
        self.0
            .send(Notice {
                summary: summary.into(),
                urgency: u8::try_from(hints.get("urgency").unwrap()).unwrap(),
            })
            .unwrap();
        1
    }
}

pub async fn notifications(bus: &Bus) -> (Connection, mpsc::UnboundedReceiver<Notice>) {
    let connection = bus.connection().await;
    let (sender, receiver) = mpsc::unbounded_channel();
    connection
        .object_server()
        .at("/org/freedesktop/Notifications", Notifications(sender))
        .await
        .unwrap();
    connection
        .request_name("org.freedesktop.Notifications")
        .await
        .unwrap();
    (connection, receiver)
}

pub async fn notice(receiver: &mut mpsc::UnboundedReceiver<Notice>) -> Notice {
    tokio::time::timeout(Duration::from_secs(4), receiver.recv())
        .await
        .unwrap()
        .unwrap()
}
