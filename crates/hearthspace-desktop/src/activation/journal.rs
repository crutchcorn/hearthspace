use serde::Serialize;
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

pub(super) fn owned_file(metadata: &Metadata) -> bool {
    metadata.is_file()
        && metadata.uid() == nix::unistd::geteuid().as_raw()
        && metadata.nlink() == 1
        && metadata.mode() & 0o022 == 0
}

pub(super) fn validate_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute()
        || !metadata.is_dir()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(io::Error::other("activation directory ownership changed"));
    }
    Ok(())
}

/// The activation flock serializes writers. Rename only a fully synced journal.
pub(super) fn replace(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing activation directory"))?;
    validate_directory(parent)?;
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::other("activation journal too large"));
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(".activation-")
        .tempfile_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    File::open(parent)?.sync_all()
}

pub(super) fn create(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()
}
