use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

use zbus::{Connection, zvariant::OwnedObjectPath};

use super::Brightness;

const SERVICE: &str = "org.freedesktop.login1";
const SESSION: &str = "/org/freedesktop/login1/session/test";

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
            .expect("dbus-daemon required for brightness tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().into(),
        }
    }

    pub fn backend(&self, root: &Path) -> Brightness {
        Brightness {
            native: true,
            root: root.into(),
            system_address: Some(self.address.clone()),
            timeout: std::time::Duration::from_millis(500),
        }
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct State {
    pub active: bool,
    pub remote: bool,
    pub seat: String,
    pub vt: u32,
    pub stalled: bool,
    pub reject: bool,
    pub requests: Vec<(String, String, u32)>,
    pub pids: Vec<u32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            active: true,
            remote: false,
            seat: "seat0".into(),
            vt: 7,
            stalled: false,
            reject: false,
            requests: Vec::new(),
            pids: Vec::new(),
        }
    }
}

type Shared = Arc<Mutex<State>>;
struct Manager(Shared);

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Manager {
    #[zbus(name = "GetSessionByPID")]
    fn get_session_by_pid(&self, pid: u32) -> OwnedObjectPath {
        self.0.lock().unwrap().pids.push(pid);
        OwnedObjectPath::try_from(SESSION).unwrap()
    }
}

struct Session {
    state: Shared,
    root: PathBuf,
}

#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl Session {
    #[zbus(property)]
    async fn active(&self) -> bool {
        let stalled = self.state.lock().unwrap().stalled;
        if stalled {
            std::future::pending::<()>().await;
        }
        self.state.lock().unwrap().active
    }

    #[zbus(property)]
    fn remote(&self) -> bool {
        self.state.lock().unwrap().remote
    }

    #[zbus(property)]
    fn seat(&self) -> (String, OwnedObjectPath) {
        (
            self.state.lock().unwrap().seat.clone(),
            OwnedObjectPath::try_from("/org/freedesktop/login1/seat/seat0").unwrap(),
        )
    }

    #[zbus(property, name = "VTNr")]
    fn vt_nr(&self) -> u32 {
        self.state.lock().unwrap().vt
    }

    fn set_brightness(&self, subsystem: &str, device: &str, value: u32) -> zbus::fdo::Result<()> {
        let mut state = self.state.lock().unwrap();
        state
            .requests
            .push((subsystem.into(), device.into(), value));
        if state.reject {
            return Err(zbus::fdo::Error::AccessDenied("Not your device".into()));
        }
        std::fs::write(self.root.join(device).join("brightness"), value.to_string())
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }
}

pub struct Logind {
    pub connection: Connection,
    pub state: Shared,
}

impl Logind {
    pub async fn start(bus: &Bus, root: &Path) -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let connection = zbus::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .serve_at("/org/freedesktop/login1", Manager(state.clone()))
            .unwrap()
            .serve_at(
                SESSION,
                Session {
                    state: state.clone(),
                    root: root.into(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        connection.request_name(SERVICE).await.unwrap();
        Self { connection, state }
    }

    pub async fn release_name(&self) {
        self.connection.release_name(SERVICE).await.unwrap();
    }
}

pub fn device(root: &Path, name: &str, kind: &str, maximum: u32, current: u32) {
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    for (file, contents) in [
        ("type", kind.into()),
        ("max_brightness", maximum.to_string()),
        ("brightness", current.to_string()),
    ] {
        std::fs::write(path.join(file), contents).unwrap();
    }
}
