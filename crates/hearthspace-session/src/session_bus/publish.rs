use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::AtomicBool,
    time::Instant,
};

use crate::session_env::SessionPaths;

use super::{
    ActivationEnvironment,
    cleanup::{self, MARKER, RECORD, Record, manager::Manager, operations},
};

pub(super) fn shared(
    paths: &SessionPaths,
    bus: &str,
    mut values: BTreeMap<String, String>,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<ActivationEnvironment> {
    let lock = operations::lock(&paths.host_runtime_dir, deadline, cancelled)?;
    if values
        .get(MARKER)
        .is_none_or(|value| value.len() != 36 || value.contains('/'))
    {
        let generation = fs::read_to_string("/proc/sys/kernel/random/uuid")?;
        values.insert(MARKER.into(), generation.trim().into());
    }
    if let Some(display) = cleanup::latest_display(&paths.session_dir, &values[MARKER])? {
        values.insert("DISPLAY".into(), display);
    }
    let mut guard = ActivationEnvironment { record: None };
    let result = operations::bounded(deadline, cancelled, async {
        let manager = Manager::connect(bus).await?;
        let previous = baseline(manager.environment().await?);
        let record = Record {
            bus: bus.into(),
            host_runtime: paths.host_runtime_dir.clone(),
            session_dir: paths.session_dir.clone(),
            published: values.clone(),
            previous,
            pending_display: None,
        };
        let bytes = serde_json::to_vec(&record)
            .map_err(|_| io::Error::other("cannot encode activation record"))?;
        if bytes.len() > 64 * 1024 {
            return Err(io::Error::other("activation record too large"));
        }
        let receipt = paths.session_dir.join(RECORD);
        if receipt.try_exists()? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "activation record already exists",
            ));
        }
        let pending = paths.session_dir.join(format!("{RECORD}.pending"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        // Recovery sees either a complete receipt or no published environment.
        fs::rename(pending, receipt)?;
        guard.record = Some(record);
        operations::remaining(deadline, cancelled)?;
        // Claim first: dbus-update updates the bus before systemd, so a failed
        // second half must still be recognizable as ours during rollback.
        manager
            .replace(
                &["WAYLAND_SOCKET".into(), "AT_SPI_BUS_ADDRESS".into()],
                &BTreeMap::from([(MARKER.into(), values[MARKER].clone())]),
            )
            .await?;
        operations::update(bus, &values, true, deadline, cancelled)?;
        let installed = manager.environment().await?;
        if values
            .iter()
            .any(|(key, value)| installed.get(key) != Some(value))
            || installed.contains_key("WAYLAND_SOCKET")
            || installed.contains_key("AT_SPI_BUS_ADDRESS")
        {
            return Err(io::Error::other(
                "activation user-manager update was incomplete",
            ));
        }
        Ok(())
    });
    // Drop rollback must reacquire this lock with a fresh cleanup deadline.
    drop(lock);
    result?;
    Ok(guard)
}

fn baseline(mut current: BTreeMap<String, String>) -> BTreeMap<String, String> {
    current.remove("WAYLAND_SOCKET");
    current.remove("AT_SPI_BUS_ADDRESS");
    let Some(previous_generation) = current.remove(MARKER).filter(|owner| !owner.is_empty()) else {
        return current;
    };
    let Some(previous_owner) = current
        .get("HEARTHSPACE_SESSION_DIR")
        .cloned()
        .filter(|directory| !directory.is_empty())
    else {
        return current;
    };
    // Flatten a still-live predecessor's baseline. Otherwise retiring the next
    // session could restore paths deleted when its predecessor ended.
    if let Ok(Some(record)) = cleanup::read_record(Path::new(&previous_owner))
        && record.published.get(MARKER) == Some(&previous_generation)
    {
        for (key, published) in record.published {
            if key != MARKER
                && (current.get(&key) == Some(&published)
                    || (key == "DISPLAY"
                        && record
                            .pending_display
                            .as_ref()
                            .is_some_and(|previous| current.get(&key) == Some(previous))))
            {
                match record.previous.get(&key) {
                    Some(value) => {
                        current.insert(key, value.clone());
                    }
                    None => {
                        current.remove(&key);
                    }
                }
            }
        }
    }
    // A crashed predecessor may have lost its record. Do not resurrect its
    // temporary directories, including entries in colon-separated search paths.
    current.retain(|key, value| {
        if matches!(key.as_str(), "PATH" | "XDG_DATA_DIRS" | "XDG_CONFIG_DIRS") {
            *value = value
                .split(':')
                .filter(|entry| !Path::new(entry).starts_with(&previous_owner))
                .collect::<Vec<_>>()
                .join(":");
            !value.is_empty()
        } else {
            !value.contains(&previous_owner)
        }
    });
    current
}
