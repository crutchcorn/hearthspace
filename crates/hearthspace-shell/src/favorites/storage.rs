use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use super::{Favorite, FavoriteError, FavoriteStore, MAX_FAVORITES, validate_id};

const MAX_FILE_BYTES: u64 = 256 * 1024;
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    favorites: Vec<Favorite>,
}

pub(super) fn load(path: PathBuf) -> Result<FavoriteStore, FavoriteError> {
    let disk_contents = read(&path)?;
    let entries = match &disk_contents {
        None => Vec::new(),
        Some(bytes) => {
            let document: Document = serde_json::from_slice(bytes)
                .map_err(|e| error(&path, format!("cannot read saved data: {e}")))?;
            if document.version != 1 {
                return Err(error(&path, "unsupported favorites format version"));
            }
            if document.favorites.len() > MAX_FAVORITES {
                return Err(error(&path, "too many saved favorites"));
            }
            let mut ids = HashSet::new();
            for entry in &document.favorites {
                validate_id(&entry.app_id)?;
                if !ids.insert(&entry.app_id) {
                    return Err(error(&path, "duplicate favorite app ID"));
                }
            }
            document.favorites
        }
    };
    Ok(FavoriteStore {
        entries,
        path: Some(path),
        disk_contents,
    })
}

pub(super) fn save(
    path: &Path,
    entries: &[Favorite],
    previous: Option<&[u8]>,
) -> Result<Vec<u8>, FavoriteError> {
    // A second shell/editor's work is preserved, including a deleted file. The
    // user can reopen the shell to reload instead of silently losing that work.
    if read(path)?.as_deref() != previous {
        return Err(error(
            path,
            "favorites changed on disk; reopen the shell to reload them",
        ));
    }
    let contents = serde_json::to_vec_pretty(&Document {
        version: 1,
        favorites: entries.to_vec(),
    })
    .map_err(|e| error(path, e))?;
    if contents.len() as u64 > MAX_FILE_BYTES {
        return Err(error(path, "saved favorites exceed the file size limit"));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| error(path, e))?;
    let (temporary, mut file) = temporary_file(parent).map_err(|e| error(path, e))?;
    let result = (|| {
        file.write_all(&contents)?;
        file.sync_all()?;
        // Recheck after the write too, before replacing the user's current file.
        if read(path).map_err(std::io::Error::other)?.as_deref() != previous {
            return Err(std::io::Error::other(
                "favorites changed on disk while saving",
            ));
        }
        fs::rename(&temporary, path)
    })();
    if let Err(cause) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error(path, cause));
    }
    // The rename committed successfully. Directory syncing is best effort: a
    // post-commit fsync error must not leave memory describing the old file.
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(contents)
}

fn read(path: &Path) -> Result<Option<Vec<u8>>, FavoriteError> {
    // Inspect first so a mistakenly configured FIFO/device cannot block the UI.
    match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(error(path, "favorites must be a regular file"));
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(error(path, e)),
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(error(path, e)),
    };
    if !file.metadata().map_err(|e| error(path, e))?.is_file() {
        return Err(error(path, "favorites must be a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error(path, e))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(error(path, "favorites file is too large"));
    }
    Ok(Some(bytes))
}

fn temporary_file(parent: &Path) -> std::io::Result<(PathBuf, File)> {
    for _ in 0..16 {
        let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".favorites-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other(
        "cannot allocate a temporary favorites file",
    ))
}

fn error(path: &Path, cause: impl std::fmt::Display) -> FavoriteError {
    FavoriteError(format!(
        "Cannot save or load favorites at {}: {cause}",
        path.display()
    ))
}
