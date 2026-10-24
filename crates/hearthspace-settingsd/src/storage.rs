use hearthspace_ipc::{Settings, settings_snapshot::SettingsSnapshot};
use std::{fs, io, io::Write, path::Path};

mod document;
mod legacy;
mod transaction;

pub(super) fn load_snapshot(path: &Path) -> io::Result<SettingsSnapshot> {
    match fs::read_to_string(path) {
        Ok(contents) => document::parse_snapshot(&contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            SettingsSnapshot::new(0, Settings::default())
        }
        Err(error) => Err(error),
    }
}

pub(super) fn load_settings_from_path(path: impl AsRef<Path>) -> io::Result<Settings> {
    load_snapshot(path.as_ref()).map(|snapshot| snapshot.settings)
}

/// A busy writer returns WouldBlock immediately. The server owns bounded retries.
pub(super) fn update(
    path: &Path,
    mutate: impl FnOnce(&mut Settings) -> io::Result<()>,
) -> io::Result<SettingsSnapshot> {
    let mut lock = transaction::try_lock(path)?;
    let previous = load_snapshot(path)?;
    let mut settings = previous.settings.clone();
    mutate(&mut settings)?;
    hearthspace_ipc::validate_monitor_configs(&settings.monitors).map_err(invalid)?;
    if settings == previous.settings && previous.revision != 0 {
        return Ok(previous);
    }
    // Every supported success reply must fit before publication. The legacy
    // percent encoding can be much larger than the typed JSON snapshot.
    if hearthspace_ipc::settings_response(settings.clone()).len()
        > hearthspace_ipc::transport::MAX_RESPONSE_BYTES
    {
        return Err(invalid("legacy settings response exceeds response limit"));
    }
    let revision = transaction::reserve_revision(&mut lock, previous.revision)?;
    let snapshot = SettingsSnapshot::new(revision, settings)?;
    if hearthspace_ipc::settings_snapshot::snapshot_response(Some(snapshot.clone()))?.len()
        > hearthspace_ipc::transport::MAX_RESPONSE_BYTES
    {
        return Err(invalid("settings snapshot exceeds response limit"));
    }
    save_snapshot(snapshot.clone(), path)?;
    Ok(snapshot)
}

#[cfg(test)]
pub(super) fn save_settings_to_path(settings: Settings, path: impl AsRef<Path>) -> io::Result<()> {
    save_snapshot(SettingsSnapshot::new(0, settings)?, path.as_ref())
}

fn save_snapshot(snapshot: SettingsSnapshot, path: &Path) -> io::Result<()> {
    let contents = document::format(snapshot)?;
    let parent = parent(path);
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(contents.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    // Rename publishes the commit. Do not report failure after it is visible.
    if let Err(error) = fs::File::open(parent).and_then(|directory| directory.sync_all()) {
        tracing::warn!(%error, "settings rename committed but directory sync failed");
    }
    Ok(())
}

pub(super) fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod revision_tests;
#[cfg(test)]
mod tests;
