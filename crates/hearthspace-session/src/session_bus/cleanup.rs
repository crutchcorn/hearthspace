//! Standalone retirement path, also compiled into the app-scope guard binary.
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

#[path = "cleanup/manager.rs"]
pub mod manager;
#[path = "cleanup/operations.rs"]
pub mod operations;

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
}

pub fn read_record(directory: &Path) -> io::Result<Option<Record>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags((nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_NONBLOCK).bits())
        .open(directory.join(RECORD))
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() {
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
    Ok(Some(record))
}

/// At most two seconds; errors retain the record for the other cleanup owner.
pub fn retire_from_session_dir(directory: &Path, expected: Option<&Record>) -> io::Result<()> {
    let deadline = Instant::now() + operations::TIMEOUT;
    let cancelled = AtomicBool::new(false);
    let loaded;
    let record = match expected {
        Some(record) => record,
        None => {
            loaded = read_record(directory)?;
            let Some(record) = loaded.as_ref() else {
                return Ok(());
            };
            record
        }
    };
    if record.session_dir != directory {
        return Err(io::Error::other("activation record directory mismatch"));
    }
    let _lock = operations::lock(&record.host_runtime, deadline, &cancelled)?;
    operations::bounded(deadline, &cancelled, async {
        let manager = manager::Manager::connect(&record.bus).await?;
        let current = manager.environment().await?;
        if current.get(MARKER) != record.published.get(MARKER) {
            return remove_record(record); // A later session owns activation.
        }
        let mut restore = BTreeMap::new();
        let mut unset = Vec::new();
        let mut bus_values = BTreeMap::new();
        for (key, published) in &record.published {
            let owned = current.get(key) == Some(published);
            let next = if owned {
                record.previous.get(key)
            } else {
                current.get(key)
            };
            bus_values.insert(
                key.clone(),
                next.cloned().unwrap_or_else(|| neutral(key, record)),
            );
            if owned {
                // Value matching adds protection against non-cooperating writers
                // between our snapshot and the manager's mutation.
                unset.push(format!("{key}={published}"));
                if let Some(value) = next {
                    restore.insert(key.clone(), value.clone());
                }
            }
        }
        // D-Bus has no get/unset API. The manager's current value is our bounded
        // source of truth, including keys changed independently since startup.
        // Repair the bus FIRST; release the manager's ownership marker LAST.
        operations::update(&record.bus, &bus_values, false, deadline, &cancelled)?;
        let latest = manager.environment().await?;
        if latest.get(MARKER) != record.published.get(MARKER) {
            return remove_record(record);
        }
        restore.retain(|key, _| latest.get(key) == record.published.get(key));
        unset.retain(|entry| {
            entry
                .split_once('=')
                .is_some_and(|(key, value)| latest.get(key).is_some_and(|current| current == value))
        });
        manager.replace(&unset, &restore).await?;
        remove_record(record)
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
