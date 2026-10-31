//! Save the standard app hint without introducing a second preference store.
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use gio::glib::{KeyFile, KeyFileFlags};

use super::DesktopApp;

mod source;
mod transaction;

pub(crate) const KEY: &str = "PrefersNonDefaultGPU";
pub(crate) const LEGACY_KEY: &str = "X-KDE-RunOnDiscreteGpu";
const GROUP: &str = "Desktop Entry";

/// Persist a next-start hint in the user's installed desktop-entry override.
///
/// Call on a worker: this locks, rereads and atomically replaces a desktop file.
/// Other keys, translations, comments and action groups are retained. No running
/// application is restarted, and false does not force an integrated GPU.
pub fn set_app_prefers_non_default_gpu(app_id: &str, enabled: bool) -> Result<DesktopApp, String> {
    let home = match env::var_os("XDG_DATA_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".local/share"))
            .ok_or("Cannot locate your application preferences.")?,
    };
    set_in_data_dirs(app_id, enabled, &home, super::paths::xdg_data_dirs())
}

fn set_in_data_dirs(
    app_id: &str,
    enabled: bool,
    data_home: &Path,
    data_dirs: Vec<PathBuf>,
) -> Result<DesktopApp, String> {
    if app_id.is_empty()
        || !app_id.ends_with(".desktop")
        || app_id.contains(['/', '\\'])
        || app_id.chars().any(char::is_control)
        || app_id.len() > 240
        || !data_home.is_absolute()
    {
        return Err("Invalid installed application ID or user data directory.".into());
    }
    let directory = data_home.join("applications");
    fs::create_dir_all(&directory).map_err(failed)?;
    let _lock = transaction::lock(&directory, app_id).map_err(failed)?;
    // Reread after acquiring the stable ID lock: another shell may have saved
    // an override while we waited. Never replace it with a stale catalog copy.
    let app = lookup(&data_dirs, app_id)?;
    let target = source::override_path(&directory, &data_dirs, &app)?;
    let before = read(&app.path)?.ok_or("This application is no longer available.")?;
    let contents = edit(&before, enabled)?;
    let parent = target.parent().ok_or("Invalid desktop-entry path.")?;
    let canonical_parent = source::prepare_parent(&directory, parent)?;
    let previous_target = read(&target)?;
    let updated = DesktopApp::from_desktop_entry(app_id.into(), target.clone(), &contents)
        .ok_or("This application is no longer available.")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(failed)?;
    temporary.write_all(contents.as_bytes()).map_err(failed)?;
    temporary.as_file().sync_all().map_err(failed)?;
    // Cooperating setters share the lock; also detect edits made by an external
    // desktop-file editor before publishing our replacement.
    if lookup(&data_dirs, app_id)?.path != app.path
        || read(&app.path)?.as_deref() != Some(before.as_str())
        || read(&target)? != previous_target
        || source::parent_identity(&directory, parent)? != canonical_parent
    {
        return Err("This application changed while saving. Try again.".into());
    }
    temporary.persist(&target).map_err(failed)?;
    // Rename committed. A directory sync failure must not report an unsaved
    // checkbox after the preference is already visible to launchers.
    if let Err(error) = fs::File::open(parent).and_then(|directory| directory.sync_all()) {
        tracing::warn!(%error, "application preference committed but directory sync failed");
    }
    Ok(updated)
}

fn lookup(data_dirs: &[PathBuf], app_id: &str) -> Result<DesktopApp, String> {
    let unavailable = || "This installed application is no longer available.".to_string();
    let path = source::find(data_dirs, app_id)?.ok_or_else(unavailable)?;
    let contents = read(&path)?.ok_or_else(unavailable)?;
    DesktopApp::from_desktop_entry(app_id.into(), path, &contents).ok_or_else(unavailable)
}

fn edit(contents: &str, enabled: bool) -> Result<String, String> {
    let document = KeyFile::new();
    document
        .load_from_data(
            contents,
            KeyFileFlags::KEEP_COMMENTS | KeyFileFlags::KEEP_TRANSLATIONS,
        )
        .map_err(failed)?;
    document.set_boolean(GROUP, KEY, enabled);
    if document.has_key(GROUP, LEGACY_KEY).map_err(failed)? {
        document.remove_key(GROUP, LEGACY_KEY).map_err(failed)?;
    }
    let contents: String = document.to_data().into();
    super::desktop_entry::check_desktop_file_size(&contents).map_err(failed)?;
    Ok(contents)
}

fn read(path: &Path) -> Result<Option<String>, String> {
    super::desktop_entry::read_desktop_file(path).map_err(failed)
}

fn failed(error: impl std::fmt::Display) -> String {
    format!("Could not save application GPU preference: {error}")
}

#[cfg(test)]
mod tests;
