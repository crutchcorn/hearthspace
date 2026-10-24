use super::super::*;
use crate::power::logind::test_support::{Bus, State, service};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

#[derive(Default)]
pub(super) struct ControlState {
    pub commands: Vec<String>,
    pub locked: bool,
    pub confirmed: bool,
    pub fail_status: bool,
    pub recovery_failed: bool,
    pub inactive: bool,
    pub fail_lock: bool,
}

pub(super) struct MockControl {
    pub path: PathBuf,
    pub state: Arc<Mutex<ControlState>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockControl {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "hearthspace-power-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let state = Arc::new(Mutex::new(ControlState::default()));
        let server_state = state.clone();
        let task = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let mut socket = BufReader::new(socket);
                let mut command = String::new();
                if socket.read_line(&mut command).await.is_err() {
                    continue;
                }
                let command = command.trim();
                let response = {
                    let mut state = server_state.lock().unwrap();
                    state.commands.push(command.into());
                    match command {
                        "lock-shield" if state.fail_lock => {
                            b"error injected lock failure\n".to_vec()
                        }
                        "prepare-sleep" | "lock-shield" => {
                            if !state.locked {
                                state.confirmed = false;
                            }
                            state.locked = true;
                            b"ok\n".to_vec()
                        }
                        "session-resume" => {
                            state.confirmed = false;
                            b"ok\n".to_vec()
                        }
                        "status" if state.fail_status => {
                            b"error injected status failure\n".to_vec()
                        }
                        "status" => {
                            let body = serde_json::to_vec(&serde_json::json!({
                                "active": !state.inactive,
                                "recovery_ready": !state.recovery_failed,
                                "locked": state.locked,
                                "lock_confirmed": state.confirmed,
                            }))
                            .unwrap();
                            let mut response = format!("ok {}\n", body.len()).into_bytes();
                            response.extend(body);
                            response
                        }
                        _ => panic!("unexpected control command: {command}"),
                    }
                };
                let _ = socket.get_mut().write_all(&response).await;
            }
        });
        Self { path, state, task }
    }
}

impl Drop for MockControl {
    fn drop(&mut self) {
        self.task.abort();
        let directory = self.path.parent().unwrap();
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(directory.join("power.json"));
        let _ = std::fs::remove_dir(directory);
    }
}

pub(super) struct Fixture {
    pub logind: Logind,
    pub state: Arc<Mutex<State>>,
    pub control: MockControl,
    _service: zbus::Connection,
    _bus: Bus,
}

impl Fixture {
    pub async fn new() -> Self {
        Self::with_state(State {
            active: true,
            ..State::default()
        })
        .await
    }

    pub async fn with_state(initial: State) -> Self {
        let bus = Bus::start();
        let state = Arc::new(Mutex::new(initial));
        let service = service(&bus, state.clone()).await;
        let logind = Logind::from_connection(bus.connection().await)
            .await
            .unwrap();
        Self {
            logind,
            state,
            control: MockControl::new(),
            _service: service,
            _bus: bus,
        }
    }

    pub async fn cycle(&self) -> Cycle {
        Cycle {
            delay: Some(self.logind.take_sleep_delay().await.unwrap()),
            lid: None,
            policy: LidPolicy::default(),
            sleeping: false,
            preparing: None,
            recovering: None,
            resume_accepted: true,
            lid_request: None,
            hint: None,
            manual: Manual::default(),
            shared: Shared::new(),
        }
    }

    pub async fn release_logind_name(&self) {
        self._service
            .release_name("org.freedesktop.login1")
            .await
            .unwrap();
    }

    pub async fn connect_logind(&self) -> io::Result<(Logind, zbus::Connection)> {
        let connection = self._bus.connection().await;
        let logind = Logind::from_connection(connection.clone())
            .await
            .map_err(io::Error::other)?;
        Ok((logind, connection))
    }

    pub async fn tick(&self, cycle: &mut Cycle, snapshot: &Snapshot, status: &mut Status) {
        cycle
            .tick_with_display_probe(snapshot, &self.logind, &self.control.path, status, || false)
            .await
            .unwrap();
    }
}

pub(super) async fn wait_status(runtime: &Path, phase: &str) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let Ok(bytes) = std::fs::read(runtime.join("power.json"))
                && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
                && value["phase"] == phase
            {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("coordinator did not reach expected state")
}
