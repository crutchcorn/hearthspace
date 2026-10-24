//! Private, bounded shell-to-supervisor power requests. The supervisor owns logind.
use std::{
    fs, io,
    os::unix::{fs::MetadataExt, fs::PermissionsExt, net::UnixListener as StdListener},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{mpsc, oneshot},
    task::JoinSet,
};

use super::action::Action;

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Reply {
    pub ok: bool,
    pub available: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub reboot_available: bool,
    pub power_off_available: bool,
    pub reboot_error: Option<String>,
    pub power_off_error: Option<String>,
}

impl Reply {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            ok: true,
            available: false,
            busy: false,
            error: Some(reason.clone()),
            reboot_error: Some(reason.clone()),
            power_off_error: Some(reason),
            ..Self::default()
        }
    }

    fn can_action(&self, action: Action) -> bool {
        !self.busy
            && match action {
                Action::Suspend => self.available,
                Action::Reboot => self.reboot_available,
                Action::PowerOff => self.power_off_available,
            }
    }

    fn action_error(&self, action: Action) -> Option<String> {
        match action {
            Action::Suspend => self.error.clone(),
            Action::Reboot => self.reboot_error.clone(),
            Action::PowerOff => self.power_off_error.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Shared {
    reply: Arc<Mutex<Reply>>,
    inflight: Arc<AtomicBool>,
    // The supervisor owns this latch across logind connections. Losing the
    // transport cannot establish that a dispatched shutdown was rejected.
    shutdown_dispatched: Arc<AtomicBool>,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            reply: Arc::new(Mutex::new(Reply::unavailable(
                "Power service is connecting",
            ))),
            inflight: Arc::new(AtomicBool::new(false)),
            shutdown_dispatched: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn update(&self, reply: Reply) {
        *self.reply.lock().unwrap() = reply;
    }

    pub fn shutdown_dispatched(&self) -> bool {
        self.shutdown_dispatched.load(Ordering::SeqCst)
    }

    pub fn set_shutdown_dispatched(&self, dispatched: bool) {
        self.shutdown_dispatched.store(dispatched, Ordering::SeqCst);
    }

    fn snapshot(&self) -> Reply {
        let mut reply = self.reply.lock().unwrap().clone();
        if self.inflight.load(Ordering::SeqCst) || self.shutdown_dispatched() {
            reply.busy = true;
            reply.available = false;
            reply.reboot_available = false;
            reply.power_off_available = false;
        }
        reply
    }
}

pub(super) struct Request {
    pub action: Action,
    reply: Option<oneshot::Sender<Reply>>,
    shared: Shared,
}

impl Request {
    #[cfg(test)]
    pub fn test_request() -> (Self, oneshot::Receiver<Reply>) {
        Self::test_action(Action::Suspend)
    }

    #[cfg(test)]
    pub fn test_action(action: Action) -> (Self, oneshot::Receiver<Reply>) {
        let shared = Shared::new();
        shared.inflight.store(true, Ordering::SeqCst);
        let (reply, receiver) = oneshot::channel();
        (
            Self {
                action,
                reply: Some(reply),
                shared,
            },
            receiver,
        )
    }

    pub fn cancelled(&self) -> bool {
        self.reply.as_ref().is_none_or(oneshot::Sender::is_closed)
    }

    pub fn finish(mut self, result: Result<(), String>) {
        let mut reply = self.shared.snapshot();
        reply.ok = result.is_ok();
        reply.error = result.err();
        if let Some(sender) = self.reply.take() {
            let _ = sender.send(reply);
        }
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        self.shared.inflight.store(false, Ordering::SeqCst);
        if let Some(sender) = self.reply.take() {
            let mut reply = Reply::unavailable(format!(
                "Power connection changed; {} was cancelled",
                self.action.label()
            ));
            reply.ok = false;
            let _ = sender.send(reply);
        }
    }
}

pub(super) struct Server {
    listener: StdListener,
    guard: SocketGuard,
    shared: Shared,
    requests: mpsc::Sender<Request>,
}

struct SocketGuard {
    path: PathBuf,
    identity: (u64, u64),
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.identity) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Server {
    pub fn bind(
        runtime: &Path,
        command: &Path,
    ) -> io::Result<(Self, Shared, mpsc::Receiver<Request>)> {
        let metadata = fs::symlink_metadata(runtime)?;
        if command.parent() != Some(runtime)
            || !metadata.is_dir()
            || metadata.uid() != nix::unistd::geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
        {
            return Err(io::Error::other(
                "power socket requires the owned private session directory",
            ));
        }
        let path = runtime.join("power-control.sock");
        // Do not remove an existing endpoint belonging to another supervisor.
        let listener = StdListener::bind(&path)?;
        let metadata = fs::symlink_metadata(&path)?;
        let guard = SocketGuard {
            path,
            identity: (metadata.dev(), metadata.ino()),
        };
        fs::set_permissions(&guard.path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let shared = Shared::new();
        let (requests, receiver) = mpsc::channel(1);
        Ok((
            Self {
                listener,
                guard,
                shared: shared.clone(),
                requests,
            },
            shared,
            receiver,
        ))
    }

    pub async fn run(self) -> io::Result<()> {
        let Self {
            listener,
            guard: _guard,
            shared,
            requests,
        } = self;
        let listener = UnixListener::from_std(listener)?;
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept(), if clients.len() < 8 => {
                    let (stream, _) = accepted?;
                    if stream.peer_cred()?.uid() != nix::unistd::geteuid().as_raw() {
                        continue;
                    }
                    let shared = shared.clone();
                    let requests = requests.clone();
                    clients.spawn(async move {
                        let _ = tokio::time::timeout(Duration::from_secs(16), serve(stream, shared, requests)).await;
                    });
                }
                _ = clients.join_next(), if !clients.is_empty() => {}
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    command: String,
}

async fn serve(
    mut stream: UnixStream,
    shared: Shared,
    requests: mpsc::Sender<Request>,
) -> io::Result<()> {
    let command = tokio::time::timeout(Duration::from_secs(1), async {
        let mut bytes = Vec::new();
        loop {
            match stream.read_u8().await? {
                b'\n' => {
                    return serde_json::from_slice::<Command>(&bytes).map_err(io::Error::other);
                }
                byte if bytes.len() < 256 => bytes.push(byte),
                _ => return Err(io::Error::other("power request is too long")),
            }
        }
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "power request timed out"))?;
    let reply = match command {
        Ok(command) if command.command == "status" => shared.snapshot(),
        Ok(command) if Action::parse(&command.command).is_some() => {
            submit(shared, requests, Action::parse(&command.command).unwrap()).await
        }
        result => {
            let reason = result
                .err()
                .map_or_else(|| "Unknown power command".into(), |error| error.to_string());
            let mut reply = Reply::unavailable(reason);
            reply.ok = false;
            reply
        }
    };
    let mut bytes = serde_json::to_vec(&reply).map_err(io::Error::other)?;
    bytes.push(b'\n');
    tokio::time::timeout(Duration::from_secs(1), stream.write_all(&bytes))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "power reply timed out"))?
}

async fn submit(shared: Shared, requests: mpsc::Sender<Request>, action: Action) -> Reply {
    let mut unavailable = shared.snapshot();
    if !unavailable.can_action(action)
        || shared
            .inflight
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
    {
        unavailable.ok = false;
        unavailable.error = Some(if unavailable.busy {
            "A power action is already in progress".into()
        } else {
            unavailable
                .action_error(action)
                .unwrap_or_else(|| format!("{} is unavailable", action.label()))
        });
        return unavailable;
    }
    let (reply, receiver) = oneshot::channel();
    let request = Request {
        action,
        reply: Some(reply),
        shared,
    };
    if let Err(error) = requests.try_send(request) {
        error
            .into_inner()
            .finish(Err("Power service is unavailable".into()));
    }
    match tokio::time::timeout(Duration::from_secs(14), receiver).await {
        Ok(Ok(reply)) => reply,
        _ => {
            let mut reply = Reply::unavailable(format!(
                "{} response timed out; it will not be retried automatically",
                action.label()
            ));
            reply.ok = false;
            reply
        }
    }
}

#[cfg(test)]
mod tests;
