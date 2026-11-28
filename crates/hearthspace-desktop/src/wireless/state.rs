//! UI-independent serial command state. Polling has no mutation side effects.
use super::*;
use tokio::sync::mpsc;
#[derive(Default)]
pub struct State {
    pub snapshot: Snapshot,
    pub busy: bool,
    pub details: Option<String>,
    pub error: Option<String>,
    pub sender: Option<mpsc::Sender<Command>>,
}
impl State {
    pub fn request(&mut self, command: Command) {
        if self.busy {
            return;
        }
        if self
            .sender
            .as_ref()
            .is_some_and(|s| s.try_send(command).is_ok())
        {
            self.busy = true;
            self.error = None;
        } else {
            self.error = Some("Wireless controls are busy. Try again.".into());
        }
    }
    pub fn receive(&mut self, update: Update) {
        self.snapshot = update.snapshot;
        if self.details.as_ref().is_some_and(|id| {
            ![&self.snapshot.wifi, &self.snapshot.bluetooth]
                .into_iter()
                .any(|r| {
                    r.as_ref()
                        .is_ok_and(|r| r.items.iter().any(|item| &item.id == id))
                })
        }) {
            self.details = None;
        }
        if update.finished {
            self.busy = false;
            self.error = update.error;
        }
    }
}
pub struct Update {
    pub snapshot: Snapshot,
    pub finished: bool,
    pub error: Option<String>,
}
pub async fn run(mut commands: mpsc::Receiver<Command>, publish: impl Fn(Update) -> bool) {
    let mut service = None;
    loop {
        if service.is_none() {
            service = Service::system().await.ok();
        }
        let snapshot = match &service {
            Some(s) => s.snapshot().await,
            None => Snapshot {
                wifi: Err("NetworkManager is unavailable.".into()),
                bluetooth: Err("Bluetooth service is unavailable.".into()),
            },
        };
        let disconnected = snapshot.wifi.is_err() && snapshot.bluetooth.is_err();
        if !publish(Update {
            snapshot,
            finished: false,
            error: None,
        }) {
            break;
        }
        if disconnected {
            service = None;
        }
        let command = tokio::select! {
            c=commands.recv()=>{let Some(c)=c else {break;};c},
            _=tokio::time::sleep(Duration::from_secs(3))=>continue,
        };
        let error = match &service {
            Some(s) => tokio::time::timeout(Duration::from_secs(150), s.execute(command))
                .await
                .unwrap_or_else(|_| Err("The wireless request timed out.".into()))
                .err(),
            None => Some("Wireless services are unavailable.".into()),
        };
        let snapshot = match &service {
            Some(s) => s.snapshot().await,
            None => Snapshot::default(),
        };
        if !publish(Update {
            snapshot,
            finished: true,
            error,
        }) {
            break;
        }
        // A new connection on the next iteration also releases any interrupted
        // application-specific pairing/discovery session.
        service = None;
    }
}

impl std::fmt::Debug for Update {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WirelessUpdate")
            .field("finished", &self.finished)
            .field("has_error", &self.error.is_some())
            .finish_non_exhaustive()
    }
}
