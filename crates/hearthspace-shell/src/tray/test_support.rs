//! Real, private D-Bus peers for protocol tests; never reads process bus env.
use super::super::{
    TrayHandle, TraySnapshot, channel,
    watcher::{WATCHER_NAME, WATCHER_PATH},
};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

pub struct Bus {
    child: Child,
    pub address: String,
}
impl Bus {
    pub fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1", "--nopidfile"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon required for tray transport tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().into(),
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

pub struct Host {
    pub handle: TrayHandle,
    pub updates: tokio::sync::watch::Receiver<TraySnapshot>,
    pub task: tokio::task::JoinHandle<()>,
}
impl Host {
    pub fn start(bus: &Bus) -> Self {
        let (handle, updates, worker) = channel(Some(bus.address.clone()));
        Self {
            handle,
            updates,
            task: tokio::spawn(worker.run()),
        }
    }
    pub async fn wait(&mut self, condition: impl Fn(&TraySnapshot) -> bool) -> TraySnapshot {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot = self.updates.borrow_and_update().clone();
                if condition(&snapshot) {
                    return snapshot;
                }
                self.updates.changed().await.expect("tray worker stopped");
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "timed out awaiting tray state; latest={:?}",
                self.updates.borrow()
            )
        })
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
pub struct State {
    pub title: String,
    pub status: String,
    pub slow: bool,
    pub prepared: bool,
    pub submenu_prepared: bool,
    pub checked: bool,
    pub actions: Vec<String>,
    pub delay_next_layout: Option<Arc<LayoutGate>>,
    pub menu_path: String,
}
#[derive(Default)]
pub struct LayoutGate {
    pub started: tokio::sync::Notify,
    pub release: tokio::sync::Notify,
}
pub type Shared = Arc<Mutex<State>>;
pub fn state() -> Shared {
    Arc::new(Mutex::new(State {
        title: "Fixture".into(),
        status: "Active".into(),
        menu_path: "/Menu".into(),
        ..State::default()
    }))
}

pub struct Item(pub Shared, pub tokio::runtime::Handle);
#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    async fn id(&self) -> &str {
        let slow = self.0.lock().unwrap().slow;
        if slow {
            let delay = {
                let _entered = self.1.enter();
                tokio::time::sleep(Duration::from_secs(2))
            };
            delay.await;
        }
        "fixture"
    }
    #[zbus(property)]
    fn title(&self) -> String {
        self.0.lock().unwrap().title.clone()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        self.0.lock().unwrap().status.clone()
    }
    #[zbus(property)]
    fn icon_name(&self) -> &str {
        "dialog-information"
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        "dialog-warning"
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![(1, 1, vec![255, 90, 100, 110])]
    }
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(self.0.lock().unwrap().menu_path.clone()).unwrap()
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }
    fn activate(&self, x: i32, y: i32) {
        self.0
            .lock()
            .unwrap()
            .actions
            .push(format!("activate:{x}:{y}"));
    }
    fn secondary_activate(&self, x: i32, y: i32) {
        self.0
            .lock()
            .unwrap()
            .actions
            .push(format!("secondary:{x}:{y}"));
    }
    fn context_menu(&self, x: i32, y: i32) {
        self.0
            .lock()
            .unwrap()
            .actions
            .push(format!("context:{x}:{y}"));
    }
    fn scroll(&self, delta: i32, orientation: &str) {
        self.0
            .lock()
            .unwrap()
            .actions
            .push(format!("scroll:{delta}:{orientation}"));
    }
}

pub struct BrokenItem;
#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl BrokenItem {
    #[zbus(property)]
    fn id(&self) -> i32 {
        42
    }
}

pub type Props = HashMap<String, OwnedValue>;
pub type Layout = (i32, Props, Vec<OwnedValue>);
pub fn text(value: &str) -> OwnedValue {
    Value::from(value).try_into().unwrap()
}
pub fn node(id: i32, label: &str, children: Vec<Layout>) -> Layout {
    let properties = Props::from([("label".into(), text(label))]);
    (
        id,
        properties,
        children
            .into_iter()
            .map(|node| Value::from(node).try_into().unwrap())
            .collect(),
    )
}

pub struct Menu(pub Shared);
#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(out_args("revision", "layout"))]
    async fn get_layout(&self, parent: i32, depth: i32, fields: Vec<String>) -> (u32, Layout) {
        assert_eq!(parent, 0);
        assert!(depth > 0);
        assert!(fields.iter().any(|field| field == "enabled"));
        let (reply, gate) = {
            let mut state = self.0.lock().unwrap();
            let mut submenu = node(
                2,
                "_Sub__menu",
                if state.submenu_prepared {
                    vec![node(3, "Child", vec![])]
                } else {
                    vec![]
                },
            );
            submenu.1.insert("children-display".into(), text("submenu"));
            let mut disabled = node(4, "Disabled", vec![]);
            disabled.1.insert("enabled".into(), OwnedValue::from(false));
            let mut check = node(5, "Checked", vec![]);
            check.1.insert("toggle-type".into(), text("checkmark"));
            check.1.insert(
                "toggle-state".into(),
                OwnedValue::from(i32::from(state.checked)),
            );
            let reply = (
                1,
                node(
                    0,
                    "",
                    vec![
                        node(
                            1,
                            if state.prepared { "Prepared" } else { "Before" },
                            vec![],
                        ),
                        submenu,
                        disabled,
                        check,
                    ],
                ),
            );
            (reply, state.delay_next_layout.take())
        };
        if let Some(gate) = gate {
            gate.started.notify_one();
            gate.release.notified().await;
        }
        reply
    }
    fn about_to_show(&self, id: i32) -> bool {
        let mut state = self.0.lock().unwrap();
        state.actions.push(format!("prepare:{id}"));
        if id == 0 {
            state.prepared = true;
        }
        if id == 2 {
            state.submenu_prepared = true;
        }
        true
    }
    fn event(&self, id: i32, event_id: &str, _data: OwnedValue, _timestamp: u32) {
        let mut state = self.0.lock().unwrap();
        state.actions.push(format!("event:{id}:{event_id}"));
        if id == 5 {
            state.checked = !state.checked;
        }
    }
}

pub async fn peer(bus: &Bus, state: Shared) -> Connection {
    let connection = bus.connection().await;
    connection
        .object_server()
        .at(
            "/StatusNotifierItem",
            Item(state.clone(), tokio::runtime::Handle::current()),
        )
        .await
        .unwrap();
    connection
        .object_server()
        .at("/Menu", Menu(state))
        .await
        .unwrap();
    connection
}

pub async fn register(connection: &Connection, service: &str) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let proxy = Proxy::new(connection, WATCHER_NAME, WATCHER_PATH, WATCHER_NAME)
                .await
                .unwrap();
            if proxy
                .call::<_, _, ()>("RegisterStatusNotifierItem", &(service,))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("tray watcher did not accept registration");
}

pub async fn signal(connection: &Connection, path: &str, interface: &str, member: &str) {
    connection
        .emit_signal(None::<&str>, path, interface, member, &())
        .await
        .unwrap();
}

pub async fn wait_action(state: &Shared, expected: &str) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if state
                .lock()
                .unwrap()
                .actions
                .iter()
                .any(|action| action == expected)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
