//! Generation-owned activation environment shared by session and compositor.
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

use serde::{Deserialize, Serialize};

#[path = "activation/manager.rs"]
pub mod manager;
#[path = "activation/operations.rs"]
pub mod operations;

mod display;
pub use display::{DISPLAY_RECORD, DisplayUpdate, initialize_display, latest_display};
mod journal;

pub const MARKER: &str = "HEARTHSPACE_ACTIVATION_OWNER";
pub const RECORD: &str = "activation-environment.json";
pub const KEYS: &[&str] = &[
    "XDG_RUNTIME_DIR",
    "WAYLAND_DISPLAY",
    "XDG_SESSION_TYPE",
    "XDG_SESSION_DESKTOP",
    "XDG_CURRENT_DESKTOP",
    "DESKTOP_SESSION",
    "DBUS_SESSION_BUS_ADDRESS",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_DIRS",
    "PATH",
    "GTK_USE_PORTAL",
    "HEARTHSPACE_SESSION_DIR",
    "HEARTHSPACE_HOST_XDG_RUNTIME_DIR",
    "HEARTHSPACE_SESSION_BUS_ADDRESS",
    "HEARTHSPACE_SETTINGS_SOCKET",
    "HEARTHSPACE_SESSION_SOCKET",
    "HEARTHSPACE_COMMAND_SOCKET",
    "HEARTHSPACE_APP_SCOPE_UNIT",
    "HEARTHSPACE_DEV",
    "HEARTHSPACE_REPO_ROOT",
    "DISPLAY",
    "PULSE_SERVER",
    "PULSE_RUNTIME_PATH",
    "PIPEWIRE_RUNTIME_DIR",
    "PIPEWIRE_REMOTE",
    "PIPEWIRE_CORE",
    "AT_SPI_BUS_ADDRESS",
    "WAYLAND_SOCKET",
    MARKER,
];

#[derive(Clone, Deserialize, Serialize)]
pub struct Record {
    pub bus: String,
    pub host_runtime: PathBuf,
    pub session_dir: PathBuf,
    pub published: BTreeMap<String, String>,
    pub previous: BTreeMap<String, String>,
    /// A crash between D-Bus and manager publication can leave either value.
    #[serde(default)]
    pub pending_display: Option<String>,
}

pub fn read_record(directory: &Path) -> io::Result<Option<Record>> {
    if let Err(error) = journal::validate_directory(directory) {
        return if error.kind() == io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags((nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_NONBLOCK).bits())
        .open(directory.join(RECORD))
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !journal::owned_file(&file.metadata()?) {
        return Err(io::Error::other("invalid activation record"));
    }
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::other("activation record too large"));
    }
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|_| io::Error::other("invalid activation record"))?;
    if record.session_dir != directory
        || !directory.is_absolute()
        || !record.host_runtime.is_absolute()
        || record
            .published
            .get(MARKER)
            .is_none_or(|marker| marker.is_empty())
        || record
            .published
            .keys()
            .chain(record.previous.keys())
            .any(|key| !KEYS.contains(&key.as_str()))
        || ["WAYLAND_SOCKET", "AT_SPI_BUS_ADDRESS"]
            .iter()
            .any(|key| record.published.contains_key(*key) || record.previous.contains_key(*key))
    {
        return Err(io::Error::other("invalid activation record ownership"));
    }
    journal::validate_directory(&record.host_runtime)?;
    Ok(Some(record))
}

/// At most two seconds; errors retain the record for the other cleanup owner.
pub fn retire_from_session_dir(directory: &Path, expected: Option<&Record>) -> io::Result<()> {
    let deadline = Instant::now() + operations::TIMEOUT;
    let cancelled = AtomicBool::new(false);
    let Some(initial) = read_record(directory)? else {
        return Ok(());
    };
    if expected
        .is_some_and(|expected| expected.published.get(MARKER) != initial.published.get(MARKER))
    {
        return Ok(());
    }
    let _lock = operations::lock(&initial.host_runtime, deadline, &cancelled)?;
    let Some(record) = read_record(directory)? else {
        return Ok(());
    };
    if record.published.get(MARKER) != initial.published.get(MARKER) {
        return Ok(());
    }
    operations::bounded(deadline, &cancelled, async {
        let manager = manager::Manager::connect(&record.bus).await?;
        let current = manager.environment().await?;
        if current.get(MARKER) != record.published.get(MARKER) {
            return remove_record(&record); // A later session owns activation.
        }
        let mut restore = BTreeMap::new();
        let mut unset = Vec::new();
        let mut bus_values = BTreeMap::new();
        let mut next_values = current.clone();
        for (key, published) in &record.published {
            let owned = current.get(key) == Some(published)
                || (key == "DISPLAY"
                    && record
                        .pending_display
                        .as_ref()
                        .is_some_and(|previous| current.get(key) == Some(previous)));
            let next = if owned {
                record.previous.get(key)
            } else {
                current.get(key)
            };
            bus_values.insert(
                key.clone(),
                next.cloned().unwrap_or_else(|| neutral(key, &record)),
            );
            if let Some(next) = next {
                next_values.insert(key.clone(), next.clone());
            } else {
                next_values.remove(key);
            }
            if owned {
                // Value matching adds protection against non-cooperating writers
                // between our snapshot and the manager's mutation.
                if let Some(value) = current.get(key) {
                    unset.push(format!("{key}={value}"));
                }
                if let Some(value) = next {
                    restore.insert(key.clone(), value.clone());
                }
            }
        }
        // An absent baseline must be unset in the manager, but D-Bus cannot
        // unset. Resolve effective audio defaults from the restored environment
        // instead of publishing invalid empty routing keys to future clients.
        bus_values.extend(
            crate::environment::audio(&record.host_runtime.to_string_lossy(), |key| {
                next_values.get(key).cloned()
            })
            .into_iter()
            .filter(|(key, _)| record.published.contains_key(key)),
        );
        // D-Bus has no get/unset API. The manager's current value is our bounded
        // source of truth, including keys changed independently since startup.
        // Repair the bus FIRST; release the manager's ownership marker LAST.
        operations::update(&record.bus, &bus_values, false, deadline, &cancelled)?;
        let latest = manager.environment().await?;
        if latest.get(MARKER) != record.published.get(MARKER) {
            return remove_record(&record);
        }
        restore.retain(|key, _| latest.get(key) == current.get(key));
        unset.retain(|entry| {
            entry
                .split_once('=')
                .is_some_and(|(key, value)| latest.get(key).is_some_and(|current| current == value))
        });
        manager.replace(&unset, &restore).await?;
        remove_record(&record)
    })
}

fn neutral(key: &str, record: &Record) -> String {
    match key {
        "XDG_RUNTIME_DIR" => record.host_runtime.to_string_lossy().into_owned(),
        "DBUS_SESSION_BUS_ADDRESS" => record.bus.clone(),
        "PATH" => "/usr/local/bin:/usr/bin:/bin".into(),
        "XDG_DATA_DIRS" => "/usr/local/share:/usr/share".into(),
        "XDG_CONFIG_DIRS" => "/etc/xdg".into(),
        _ => String::new(),
    }
}

fn remove_record(record: &Record) -> io::Result<()> {
    // A reused --session-dir may now hold a later generation's receipt.
    if read_record(&record.session_dir)?
        .is_some_and(|current| current.published.get(MARKER) != record.published.get(MARKER))
    {
        return Ok(());
    }
    match fs::remove_file(record.session_dir.join(RECORD)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
