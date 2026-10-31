//! Resolve only the requested desktop ID, preserving catalog path precedence.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::{DesktopApp, failed};

pub(super) fn find(data_dirs: &[PathBuf], id: &str) -> Result<Option<PathBuf>, String> {
    for directory in data_dirs {
        if let Some(path) = find_in(&directory.join("applications"), id).map_err(failed)? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn find_in(directory: &Path, id: &str) -> io::Result<Option<PathBuf>> {
    // A desktop ID flattens directory separators to hyphens. Only matching
    // prefixes can contain this ID; unrelated entries are never opened.
    // Shorter prefix components sort first, matching the catalog's Path sort.
    for (index, _) in id.match_indices('-') {
        let component = &id[..index];
        if matches!(component, "" | "." | "..") {
            continue;
        }
        let nested = directory.join(component);
        if nested.is_dir()
            && let Some(path) = find_in(&nested, &id[index + 1..])?
        {
            return Ok(Some(path));
        }
    }
    let path = directory.join(id);
    // Match catalog discovery: a basename of `.desktop` has no extension and
    // must not shadow the visible `vendor-.desktop` entry with the same ID.
    if !path
        .extension()
        .is_some_and(|extension| extension == "desktop")
    {
        return Ok(None);
    }
    match fs::symlink_metadata(&path) {
        Ok(_) if !path.is_dir() => Ok(Some(path)),
        Ok(_) => Ok(None),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn override_path(
    directory: &Path,
    data_dirs: &[PathBuf],
    app: &DesktopApp,
) -> Result<PathBuf, String> {
    // GIO derives the activation bus name from the basename, not the flattened
    // catalog ID. Preserve the relative path even for the first user override.
    for data_dir in data_dirs {
        let applications = data_dir.join("applications");
        if super::super::desktop_entry::desktop_entry_id(&applications, &app.path).as_deref()
            == Some(app.id.as_str())
        {
            let relative = app.path.strip_prefix(&applications).map_err(failed)?;
            return Ok(directory.join(relative));
        }
    }
    Err("Cannot locate the application's installation directory.".into())
}

pub(super) fn prepare_parent(directory: &Path, parent: &Path) -> Result<PathBuf, String> {
    let relative = parent.strip_prefix(directory).map_err(failed)?;
    let mut component_path = directory.to_path_buf();
    for component in relative.components() {
        component_path.push(component);
        // Create one component at a time, checking it before descending. A
        // preexisting symlink must never redirect mkdir into another ID's tree.
        match fs::create_dir(&component_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(failed(error)),
        }
        check_directory(&component_path)?;
    }
    parent_identity(directory, parent)
}

fn check_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(failed)?;
    if metadata.file_type().is_symlink() {
        return Err(
            "The application override directory contains a symlink. Resolve it before saving."
                .into(),
        );
    }
    if !metadata.is_dir() {
        return Err("The application override parent must be a directory.".into());
    }
    Ok(())
}

pub(super) fn parent_identity(directory: &Path, parent: &Path) -> Result<PathBuf, String> {
    let relative = parent.strip_prefix(directory).map_err(failed)?;
    let mut component_path = directory.to_path_buf();
    for component in relative.components() {
        component_path.push(component);
        check_directory(&component_path)?;
    }
    let canonical_directory = directory.canonicalize().map_err(failed)?;
    let canonical_parent = parent.canonicalize().map_err(failed)?;
    if !canonical_parent.starts_with(&canonical_directory) {
        return Err("The application override directory points outside your applications.".into());
    }
    Ok(canonical_parent)
}
