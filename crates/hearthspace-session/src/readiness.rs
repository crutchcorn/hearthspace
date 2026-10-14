use std::{
    io,
    os::{fd::AsRawFd, unix::net::UnixStream},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use nix::{
    errno::Errno,
    sys::socket::{AddressFamily, SockFlag, SockType, UnixAddr, connect, socket},
};

/// AF_UNIX connect can block on a full listen backlog; nonblocking sockets keep
/// that path within the same absolute startup budget as reads and child polling.
pub(crate) fn connect_until(
    path: &Path,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<UnixStream> {
    let address = UnixAddr::new(path)?;
    loop {
        remaining(deadline, cancelled)?;
        let fd = socket(
            AddressFamily::Unix,
            SockType::Stream,
            SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
            None,
        )?;
        match connect(fd.as_raw_fd(), &address) {
            Ok(()) => {
                let stream = UnixStream::from(fd);
                stream.set_nonblocking(false)?;
                return Ok(stream);
            }
            Err(Errno::EAGAIN | Errno::EINPROGRESS) => {
                thread::sleep(remaining(deadline, cancelled)?.min(Duration::from_millis(10)));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub(crate) fn remaining(deadline: Instant, cancelled: &AtomicBool) -> io::Result<Duration> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "session shutdown requested",
        ));
    }
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "absolute startup deadline exceeded",
            )
        })
}

pub(crate) fn check_startup_deadline(
    deadline: Instant,
    shutdown_requested: bool,
    child: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if shutdown_requested {
        return Err(format!("shutdown requested while waiting for {child}").into());
    }
    if Instant::now() >= deadline {
        return Err(format!("absolute startup deadline exceeded waiting for {child}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn deadline_is_absolute_even_if_child_keeps_logging() {
        let past = Instant::now() - Duration::from_secs(1);
        assert!(check_startup_deadline(past, false, "noisy child").is_err());
    }

    #[test]
    fn full_unix_listen_backlog_cannot_block_connect() {
        use nix::sys::socket::{Backlog, listen};
        use std::os::unix::net::UnixListener;
        let path = std::env::temp_dir().join(format!(
            "hearthspace-full-backlog-{}.sock",
            std::process::id()
        ));
        let listener = UnixListener::bind(&path).unwrap();
        listen(&listener, Backlog::new(0).unwrap()).unwrap();
        let _queued = UnixStream::connect(&path).unwrap();
        let started = Instant::now();
        let error = connect_until(
            &path,
            started + Duration::from_millis(40),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn cancellation_interrupts_startup() {
        let future = Instant::now() + Duration::from_secs(60);
        assert!(check_startup_deadline(future, true, "child").is_err());
        assert!(check_startup_deadline(future, false, "child").is_ok());
    }
}
