//! Filesystem work and Linux application associations, always called off the UI thread.
use super::{DesktopItem, DesktopItemKind};
use gio::prelude::*;
use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

mod properties;
mod transfer;
pub use properties::properties;
pub use transfer::paste;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct OpenWithApp {
    pub id: String,
    pub name: String,
}

pub fn paths(items: &[DesktopItem]) -> Result<Vec<PathBuf>, String> {
    items
        .iter()
        .map(|item| match &item.kind {
            DesktopItemKind::File { path } | DesktopItemKind::Folder { path } => Ok(path.clone()),
            DesktopItemKind::App { .. } => {
                Err("This action is available for files and folders.".into())
            }
        })
        .collect()
}

pub fn valid_name(name: &str) -> Result<(), String> {
    let mut components = Path::new(name).components();
    if name.is_empty()
        || name.contains('\0')
        || name.contains('/')
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        Err("Enter a name without /; . and .. are not allowed.".into())
    } else {
        Ok(())
    }
}

pub fn new_folder(directory: &Path, name: &str) -> Result<(), String> {
    valid_name(name)?;
    fs::create_dir(directory.join(name)).map_err(|e| file_error("Could not create folder", e))
}

pub fn rename(path: &Path, name: &str) -> Result<(), String> {
    valid_name(name)?;
    let source = gio::File::for_path(path);
    let parent = source.parent().ok_or("The source has no parent folder.")?;
    // Keep GIO's display-name encoding, but publish atomically: its local
    // set_display_name implementation checks existence before ordinary rename.
    let destination = parent
        .child_for_display_name(name)
        .map_err(|e| format!("Could not rename: {e}"))?
        .path()
        .ok_or("The destination is not a local file.")?;
    if path.file_name() == destination.file_name() {
        return fs::symlink_metadata(path)
            .map(|_| ())
            .map_err(|e| format!("Could not rename: {e}"));
    }
    transfer::rename_without_replace(path, &destination)
        .map_err(|e| file_error("Could not rename", e))
}

fn file_error(operation: &str, error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        "A file or folder with that name already exists.".into()
    } else {
        format!("{operation}: {error}")
    }
}

pub fn trash(paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        gio::File::for_path(path)
            .trash(gio::Cancellable::NONE)
            .map_err(|e| format!("Could not move {} to Trash: {e}", path.display()))?;
    }
    Ok(())
}

fn content_type(path: &Path) -> Result<String, String> {
    let info = gio::File::for_path(path)
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .map_err(|e| e.to_string())?;
    Ok(info
        .content_type()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/octet-stream".into()))
}

pub fn open_with_apps(path: &Path) -> Result<Vec<OpenWithApp>, String> {
    let mut apps: Vec<_> = gio::AppInfo::all_for_type(&content_type(path)?)
        .into_iter()
        .filter(|app| app.supports_files() || app.supports_uris())
        .filter_map(|app| {
            Some(OpenWithApp {
                id: app.id()?.to_string(),
                name: app.display_name().to_string(),
            })
        })
        .collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);
    Ok(apps)
}

pub fn open_with(paths: &[PathBuf], app_id: &str, x_display: Option<&str>) -> Result<(), String> {
    let desktop_file = gio_unix::DesktopAppInfo::new(app_id)
        .and_then(|app| app.filename())
        .ok_or("This application is no longer available.")?;
    // GIO's D-Bus activation reads the process environment, not the launch
    // context's child environment. Run it in the same isolated session as apps.
    super::run_opener(open_with_command(&desktop_file, paths, x_display)?)
}

fn open_with_command(
    desktop_file: &Path,
    paths: &[PathBuf],
    x_display: Option<&str>,
) -> Result<Command, String> {
    if paths.is_empty() || paths.iter().any(|path| !path.is_absolute()) {
        return Err("Select local files or folders to open.".into());
    }
    let mut command = crate::app_catalog::launch_command(
        &["gio".into(), "launch".into(), "--".into()],
        &hearthspace_common::wayland_display_name(),
        &[],
        None,
    )?;
    command.arg(desktop_file).args(paths);
    if let Some(display) = x_display {
        command.env("DISPLAY", display);
    }
    Ok(command)
}
