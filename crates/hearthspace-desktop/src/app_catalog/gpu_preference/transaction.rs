//! A stable per-desktop-ID lock survives atomic replacement and nested paths.
use std::{
    fs::{File, OpenOptions, TryLockError},
    io,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub(super) fn lock(directory: &Path, id: &str) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(directory.join(format!(".{id}.lock")))?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other(
            "application preference lock is not a regular file",
        ));
    }
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::Error(error)) => return Err(error),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                #[cfg(test)]
                CONTENTION_OBSERVER.with(|observer| {
                    if let Some(observer) = observer.borrow_mut().take() {
                        observer();
                    }
                });
                thread::sleep(Duration::from_millis(5));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(io::Error::other(
                    "another application preference save is in progress; try again",
                ));
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    static CONTENTION_OBSERVER: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Synchronize a regression at a real failed lock acquisition, not thread start.
#[cfg(test)]
pub(super) fn observe_contention(observer: impl FnOnce() + 'static) {
    CONTENTION_OBSERVER.with(|slot| *slot.borrow_mut() = Some(Box::new(observer)));
}
