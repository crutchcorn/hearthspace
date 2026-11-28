//! Real files and shortcuts from the configured XDG desktop folder.
//!
//! Discovery reads directory metadata only. File contents and application icons
//! are never decoded on the UI thread or as part of a directory refresh.
use std::{
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub mod actions;
pub(crate) mod artwork;
use hearthspace_desktop::file_launch;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopItem {
    pub id: String,
    pub title: String,
    pub kind: DesktopItemKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopItemKind {
    App {
        app_id: String,
        icon: Option<String>,
    },
    Folder {
        path: PathBuf,
    },
    File {
        path: PathBuf,
    },
}

/// `dirs` already participates in the workspace dependency graph and understands
/// localized XDG user directories, escaped paths, and disabled desktop folders.
pub fn desktop_directory() -> Option<PathBuf> {
    let directory = dirs::desktop_dir()?;
    (directory.is_absolute() && Some(&directory) != dirs::home_dir().as_ref()).then_some(directory)
}

pub fn discover(directory: Option<&Path>) -> io::Result<Vec<DesktopItem>> {
    match directory {
        Some(directory) => files(directory),
        None => Ok(Vec::new()),
    }
}

fn files(directory: &Path) -> io::Result<Vec<DesktopItem>> {
    if !directory.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "desktop directory must be absolute",
        ));
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut items = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if name.as_bytes().starts_with(b".") {
            continue;
        }
        // Follow links for presentation/opening, but never enumerate directories
        // recursively. Broken links and special files are not launch targets.
        let path = entry.path();
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let kind = if metadata.is_dir() {
            DesktopItemKind::Folder { path }
        } else if metadata.is_file() {
            DesktopItemKind::File { path }
        } else {
            continue;
        };
        let encoded: String = name
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        items.push(DesktopItem {
            id: format!("file:{encoded}"),
            title: name.to_string_lossy().into_owned(),
            kind,
        });
    }
    items.sort_by(|a, b| {
        let rank = |item: &DesktopItem| !matches!(item.kind, DesktopItemKind::Folder { .. });
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(items)
}

/// Select with GIO on the desktop worker, then launch through the compositor's
/// supervised environment. File URIs preserve native filename bytes over IPC.
pub fn open_path(socket: &Path, path: &Path, output: Option<&str>) -> Result<(), String> {
    file_launch::Request::for_default(path, output)?.send(socket)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod launch_tests;

#[cfg(test)]
pub(crate) mod test_support;
