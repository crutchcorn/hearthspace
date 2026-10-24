//! One persistent lock inode for every daemon sharing the preference path.
use nix::fcntl::OFlag;
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Seek, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub(super) fn try_lock(path: &Path) -> io::Result<File> {
    fs::create_dir_all(super::parent(path))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK).bits())
        .open(lock_path(path))?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("settings lock must be a regular file"));
    }
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(io::ErrorKind::WouldBlock.into()),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

pub(super) fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

// Reserve before saving. A failed write may leave a gap, but never publishes a
// revision; the high-water mark survives hand edits/removal of the TOML revision.
pub(super) fn reserve_revision(lock: &mut File, previous: u64) -> io::Result<u64> {
    let mut bytes = Vec::new();
    (&mut *lock).take(9).read_to_end(&mut bytes)?;
    let high_water = match bytes.len() {
        0 => 0,
        8 => u64::from_le_bytes(bytes.try_into().unwrap()),
        _ => return Err(io::Error::other("invalid settings revision lock file")),
    };
    let next = previous
        .max(high_water)
        .checked_add(1)
        .filter(|revision| *revision <= i64::MAX as u64)
        .ok_or_else(|| io::Error::other("settings revision exhausted"))?;
    lock.rewind()?;
    lock.write_all(&next.to_le_bytes())?;
    lock.sync_all()?;
    Ok(next)
}
