//! Live DISPLAY publication uses the same lock, generation and crash journal.
use super::{MARKER, RECORD, journal, manager::Manager, operations, read_record};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

pub const DISPLAY_RECORD: &str = "activation-display.json";

#[derive(Deserialize, Serialize)]
struct DisplayState {
    generation: String,
    host_runtime: PathBuf,
    bus: String,
    private: bool,
    host_device: u64,
    host_inode: u64,
    uid: u32,
    device: u64,
    inode: u64,
    revision: u64,
    display: String,
}

/// Created before the compositor starts; an old compositor cannot claim a reused path.
pub fn initialize_display(
    host: &Path,
    session: &Path,
    bus: &str,
    private: bool,
    generation: &str,
) -> io::Result<()> {
    journal::validate_directory(host)?;
    journal::validate_directory(session)?;
    let metadata = fs::symlink_metadata(session)?;
    let host_metadata = fs::symlink_metadata(host)?;
    let state = DisplayState {
        generation: generation.into(),
        host_runtime: host.into(),
        bus: bus.into(),
        private,
        host_device: host_metadata.dev(),
        host_inode: host_metadata.ino(),
        uid: metadata.uid(),
        device: metadata.dev(),
        inode: metadata.ino(),
        revision: 0,
        display: String::new(),
    };
    journal::create(&session.join(DISPLAY_RECORD), &state)
}

fn load(session: &Path) -> io::Result<Option<DisplayState>> {
    if let Err(error) = journal::validate_directory(session) {
        return if error.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags((nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_NONBLOCK).bits())
        .open(session.join(DISPLAY_RECORD))
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !journal::owned_file(&file.metadata()?) {
        return Err(io::Error::other("invalid display ownership record"));
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 {
        return Err(io::Error::other("display ownership record too large"));
    }
    let state: DisplayState = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    let metadata = fs::symlink_metadata(session)?;
    if state.generation.is_empty()
        || metadata.uid() != state.uid
        || metadata.dev() != state.device
        || metadata.ino() != state.inode
    {
        return Err(io::Error::other("display session ownership changed"));
    }
    journal::validate_directory(&state.host_runtime)?;
    let host = fs::symlink_metadata(&state.host_runtime)?;
    if host.dev() != state.host_device || host.ino() != state.host_inode {
        return Err(io::Error::other("activation host runtime was replaced"));
    }
    Ok(Some(state))
}

/// Caller holds the activation lock while building initial publication.
pub fn latest_display(session: &Path, generation: &str) -> io::Result<Option<String>> {
    Ok(load(session)?
        .filter(|state| state.generation == generation)
        .map(|state| state.display))
}

#[derive(Clone, Debug)]
pub struct DisplayUpdate {
    pub session: PathBuf,
    pub generation: String,
    pub revision: u64,
    pub display: Option<String>,
}

impl DisplayUpdate {
    pub fn publish(&self, deadline: Instant, cancelled: &AtomicBool) -> io::Result<()> {
        self.apply(deadline, cancelled, false)
    }

    /// Launching must not race initial publication or a generation's retirement.
    pub fn synchronize(&self, deadline: Instant, cancelled: &AtomicBool) -> io::Result<()> {
        self.apply(deadline, cancelled, true)
    }

    fn apply(&self, deadline: Instant, cancelled: &AtomicBool, launching: bool) -> io::Result<()> {
        let initial =
            load(&self.session)?.ok_or_else(|| io::Error::other("display session retired"))?;
        if initial.generation != self.generation {
            return Err(io::Error::other("display generation changed"));
        }
        let _lock = operations::lock(&initial.host_runtime, deadline, cancelled)?;
        let mut state =
            load(&self.session)?.ok_or_else(|| io::Error::other("display session retired"))?;
        if state.generation != self.generation
            || state.host_runtime != initial.host_runtime
            || state.revision > self.revision
        {
            return Err(io::Error::other("display publication was superseded"));
        }
        let display = self.display.clone().unwrap_or_default();
        if !display.is_empty()
            && (!display.starts_with(':')
                || display.len() < 2
                || !display[1..].bytes().all(|byte| byte.is_ascii_digit())
                || display.len() > 12)
        {
            return Err(io::Error::other("invalid compositor X11 display"));
        }
        if state.revision == self.revision && state.display != display {
            return Err(io::Error::other("display revision was reused"));
        }
        state.revision = self.revision;
        state.display = display.clone();
        operations::remaining(deadline, cancelled)?;
        journal::replace(&self.session.join(DISPLAY_RECORD), &state)?;
        if state.private {
            return operations::update(
                &state.bus,
                &BTreeMap::from([("DISPLAY".into(), display)]),
                false,
                deadline,
                cancelled,
            );
        }
        let Some(mut record) = read_record(&self.session)? else {
            return if launching {
                Err(io::Error::other("session activation is not published"))
            } else {
                Ok(())
            };
        };
        if record.published.get(MARKER) != Some(&self.generation)
            || record.bus != state.bus
            || record.host_runtime != state.host_runtime
        {
            return Err(io::Error::other("activation generation changed"));
        }
        operations::bounded(deadline, cancelled, async {
            let manager = Manager::connect(&record.bus).await?;
            let current = manager.environment().await?;
            if current.get(MARKER) != Some(&self.generation) {
                return Err(io::Error::other("another session owns activation"));
            }
            if record.published.get("DISPLAY") == Some(&display)
                && record.pending_display.is_none()
                && current.get("DISPLAY") == Some(&display)
            {
                return Ok(());
            }
            let previous = record.published.get("DISPLAY").cloned().unwrap_or_default();
            if current.get("DISPLAY") != Some(&previous)
                && !record
                    .pending_display
                    .as_ref()
                    .is_some_and(|pending| current.get("DISPLAY") == Some(pending))
            {
                return Err(io::Error::other("activation display changed independently"));
            }
            record.pending_display = current.get("DISPLAY").cloned();
            record.published.insert("DISPLAY".into(), display.clone());
            journal::replace(&self.session.join(RECORD), &record)?;
            operations::remaining(deadline, cancelled)?;
            // The journal covers both values until both activation targets agree.
            operations::update(
                &record.bus,
                &BTreeMap::from([("DISPLAY".into(), display.clone())]),
                false,
                deadline,
                cancelled,
            )?;
            let latest = manager.environment().await?;
            if latest.get(MARKER) != Some(&self.generation)
                || latest.get("DISPLAY") != current.get("DISPLAY")
            {
                return Err(io::Error::other(
                    "activation owner changed during display update",
                ));
            }
            manager
                .replace(&[], &BTreeMap::from([("DISPLAY".into(), display)]))
                .await?;
            record.pending_display = None;
            journal::replace(&self.session.join(RECORD), &record)
        })
    }
}
