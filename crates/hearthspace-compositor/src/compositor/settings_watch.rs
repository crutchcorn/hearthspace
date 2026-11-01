//! settingsd owns commits; one worker delivers only committed projections to Calloop.
use super::{CalloopData, shell_integration::ControlReply};
use hearthspace_ipc::settings_snapshot::{self, SettingsCursor, SettingsSnapshot};
use smithay::reexports::calloop::{
    LoopHandle,
    ping::{self, Ping},
};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const MAX_REFRESHES: usize = 8;
const WAIT: Duration = Duration::from_millis(500);

type Reply = mpsc::Sender<ControlReply>;

#[derive(Default)]
struct Mailbox {
    latest: Option<SettingsSnapshot>,
    completed: Vec<(Reply, Result<(), String>)>,
    pending: usize,
}

pub(super) struct Subscription {
    stop: Arc<AtomicBool>,
    mailbox: Arc<Mutex<Mailbox>>,
    requests: mpsc::SyncSender<Reply>,
    worker: thread::Thread,
}

impl Subscription {
    #[cfg(test)]
    pub(in crate::compositor) fn disconnected() -> Self {
        let (requests, _) = mpsc::sync_channel(1);
        Self {
            stop: Arc::new(AtomicBool::new(true)),
            mailbox: Default::default(),
            requests,
            worker: thread::current(),
        }
    }

    pub fn refresh(&self) -> ControlReply {
        let (reply, receive) = mpsc::channel();
        let mut mailbox = self.mailbox.lock().unwrap();
        if self.stop.load(Ordering::Acquire) || mailbox.pending >= MAX_REFRESHES {
            return ControlReply::Err("settings refresh is busy".into());
        }
        match self.requests.try_send(reply) {
            Ok(()) => mailbox.pending += 1,
            Err(_) => return ControlReply::Err("settings refresh is unavailable".into()),
        }
        self.worker.unpark();
        ControlReply::Deferred(receive)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.unpark();
        // A pending bounded socket operation exits in at most WAIT + 500 ms.
        // Do not join a transport worker from an input/render callback.
    }
}

pub(super) fn subscribe(handle: &LoopHandle<'static, CalloopData>) -> io::Result<Subscription> {
    let socket = hearthspace_ipc::settings_socket_path()?;
    let mailbox = Arc::new(Mutex::new(Mailbox::default()));
    let (ping, source) = ping::make_ping()?;
    let complete = mailbox.clone();
    handle
        .insert_source(source, move |(), _, data| {
            let (snapshot, replies) = {
                let mut mailbox = complete.lock().unwrap();
                let snapshot = mailbox.latest.take();
                let replies = std::mem::take(&mut mailbox.completed);
                mailbox.pending -= replies.len();
                (snapshot, replies)
            };
            if let Some(snapshot) = snapshot {
                let changed = data.state.apply_settings(snapshot.settings);
                tracing::debug!(
                    revision = snapshot.revision,
                    output_layout_changed = changed.output_layout_changed,
                    background_changed = changed.background_changed,
                    appearance_changed = changed.appearance_changed,
                    "applied committed compositor settings"
                );
            }
            // A manual refresh acknowledges only after applying its snapshot.
            for (reply, result) in replies {
                let _ = reply.send(result.map_or_else(ControlReply::Err, |_| ControlReply::Ok));
            }
        })
        .map_err(|error| io::Error::other(error.to_string()))?;
    let stop = Arc::new(AtomicBool::new(false));
    let (requests, receive) = mpsc::sync_channel(MAX_REFRESHES);
    let worker_stop = stop.clone();
    let worker_mailbox = mailbox.clone();
    let worker = thread::Builder::new()
        .name("settings-watch".into())
        .spawn(move || {
            run(socket, worker_stop, receive, worker_mailbox, ping);
        })?;
    Ok(Subscription {
        stop,
        mailbox,
        requests,
        worker: worker.thread().clone(),
    })
}

fn run(
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    requests: mpsc::Receiver<Reply>,
    mailbox: Arc<Mutex<Mailbox>>,
    ping: Ping,
) {
    let mut cursor: Option<SettingsCursor> = None;
    let mut failed = false;
    while !stop.load(Ordering::Acquire) {
        let replies: Vec<_> = requests.try_iter().collect();
        let result = match &cursor {
            Some(cursor) if replies.is_empty() => {
                settings_snapshot::client_wait_for_change_at(&socket, cursor, WAIT)
            }
            _ => settings_snapshot::client_get_snapshot_at(&socket).map(Some),
        };
        let failure = result.is_err();
        let (snapshot, completion) = match result {
            Ok(snapshot) => {
                if let Some(snapshot) = &snapshot {
                    cursor = Some(snapshot.cursor());
                }
                (snapshot, Ok(()))
            }
            Err(error) => {
                if !failed {
                    tracing::debug!(%error, "settings subscription disconnected; retaining committed state");
                }
                cursor = None;
                (None, Err(format!("settings refresh failed: {error}")))
            }
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        if snapshot.is_some() || !replies.is_empty() {
            let mut mailbox = mailbox.lock().unwrap();
            if snapshot.is_some() {
                mailbox.latest = snapshot;
            }
            mailbox
                .completed
                .extend(replies.into_iter().map(|reply| (reply, completion.clone())));
            drop(mailbox);
            ping.ping();
        }
        failed = failure;
        if failed && !stop.load(Ordering::Acquire) {
            thread::park_timeout(WAIT);
        }
    }
}

#[cfg(test)]
mod tests;
