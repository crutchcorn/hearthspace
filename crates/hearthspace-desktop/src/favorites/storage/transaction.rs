use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const WAIT_LIMIT: Duration = Duration::from_millis(500);

pub(super) fn lock(path: &Path) -> io::Result<File> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(lock_path(path))?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("favorites lock must be a regular file"));
    }
    let deadline = Instant::now() + WAIT_LIMIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::Error(error)) => return Err(error),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "another favorites save is in progress; try again",
                ));
            }
        }
    }
}

// Keep this file after unlocking: unlinking would let concurrent writers lock
// different inodes. Closing the descriptor releases the advisory lock.
pub(super) fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests;
