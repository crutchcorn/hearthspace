use super::*;
use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

pub(in crate::power) const SESSION_PATH: &str = "/org/freedesktop/login1/session/test";

pub(in crate::power) struct Bus {
    child: Child,
    address: String,
}

impl Bus {
    pub(in crate::power) fn start() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1", "--nopidfile"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is required for isolated logind transport tests");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            child,
            address: address.trim().to_owned(),
        }
    }

    pub(in crate::power) async fn connection(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
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

#[derive(Default)]
pub(in crate::power) struct State {
    pub(in crate::power) active: bool,
    pub(in crate::power) remote: bool,
    pub(in crate::power) closed: bool,
    pub(in crate::power) docked: bool,
    pub(in crate::power) preparing: bool,
    pub(in crate::power) locked: bool,
    pub(in crate::power) slow_suspend: bool,
    pub(in crate::power) suspend_calls: Vec<bool>,
    pub(in crate::power) inhibitors: Vec<(String, String, UnixStream)>,
}

struct Manager(Arc<Mutex<State>>);

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Manager {
    #[zbus(name = "GetSessionByPID")]
    fn get_session_by_pid(&self, pid: u32) -> zbus::fdo::Result<OwnedObjectPath> {
        if pid != std::process::id() {
            return Err(zbus::fdo::Error::Failed("wrong caller PID".into()));
        }
        Ok(OwnedObjectPath::try_from(SESSION_PATH).unwrap())
    }

    #[zbus(property)]
    fn lid_closed(&self) -> bool {
        self.0.lock().unwrap().closed
    }

    #[zbus(property)]
    fn docked(&self) -> bool {
        self.0.lock().unwrap().docked
    }

    #[zbus(property)]
    fn preparing_for_sleep(&self) -> bool {
        self.0.lock().unwrap().preparing
    }

    #[zbus(property)]
    fn inhibit_delay_max_u_sec(&self) -> u64 {
        5_000_000
    }

    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::zvariant::OwnedFd {
        assert_eq!(who, "Hearthspace");
        assert!(!why.is_empty());
        let (read, write) = UnixStream::pair().unwrap();
        read.set_nonblocking(true).unwrap();
        self.0
            .lock()
            .unwrap()
            .inhibitors
            .push((what.into(), mode.into(), read));
        OwnedFd::from(write).into()
    }

    async fn suspend(&self, interactive: bool) {
        let slow = {
            let mut state = self.0.lock().unwrap();
            state.suspend_calls.push(interactive);
            state.slow_suspend
        };
        if slow {
            std::future::pending::<()>().await;
        }
    }
}

struct Session(Arc<Mutex<State>>);

#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl Session {
    #[zbus(property)]
    fn id(&self) -> &str {
        "test"
    }

    #[zbus(property)]
    fn active(&self) -> bool {
        self.0.lock().unwrap().active
    }

    #[zbus(property)]
    fn remote(&self) -> bool {
        self.0.lock().unwrap().remote
    }

    #[zbus(property)]
    fn seat(&self) -> (String, OwnedObjectPath) {
        (
            "seat0".into(),
            OwnedObjectPath::try_from("/org/freedesktop/login1/seat/seat0").unwrap(),
        )
    }

    #[zbus(property, name = "VTNr")]
    fn vt_nr(&self) -> u32 {
        7
    }

    fn set_locked_hint(&self, locked: bool) {
        self.0.lock().unwrap().locked = locked;
    }
}

pub(in crate::power) async fn service(bus: &Bus, state: Arc<Mutex<State>>) -> Connection {
    zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .serve_at(MANAGER_PATH, Manager(state.clone()))
        .unwrap()
        .serve_at(SESSION_PATH, Session(state))
        .unwrap()
        .name(SERVICE)
        .unwrap()
        .build()
        .await
        .unwrap()
}

pub(in crate::power) async fn signal(connection: &Connection, preparing: bool) {
    connection
        .emit_signal(
            None::<&str>,
            MANAGER_PATH,
            MANAGER_INTERFACE,
            "PrepareForSleep",
            &(preparing,),
        )
        .await
        .unwrap();
}
