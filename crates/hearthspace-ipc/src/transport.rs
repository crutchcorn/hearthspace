//! Bounded line exchanges for small local control and settings messages.
use std::{
    io::{self, Read, Write},
    net::Shutdown,
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

pub const MAX_REQUEST_BYTES: usize = 16 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;

pub mod launch;
mod payload;
pub use payload::request_payload;

/// Exchange exactly one newline-terminated message within one absolute budget.
/// The response does not require EOF: services may retain their connection.
pub fn request_line(path: &Path, request: &str, timeout: Duration) -> io::Result<String> {
    if request.len() > MAX_REQUEST_BYTES
        || !request.ends_with('\n')
        || request[..request.len() - 1].contains('\n')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "request must be one bounded newline-terminated line",
        ));
    }
    let deadline = Instant::now() + timeout;
    let mut stream = connect_until(path, deadline)?;
    write_until(&mut stream, request.as_bytes(), deadline)?;
    stream.shutdown(Shutdown::Write)?;
    read_line_until(&mut stream, MAX_RESPONSE_BYTES, deadline)
}

/// Even connecting must be nonblocking: a full AF_UNIX backlog otherwise blocks.
pub fn connect_until(path: &Path, deadline: Instant) -> io::Result<UnixStream> {
    let address = UnixAddr::new(path)?;
    loop {
        remaining(deadline)?;
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
                thread::sleep(remaining(deadline)?.min(Duration::from_millis(1)));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub fn read_line_until(
    stream: &mut UnixStream,
    max_bytes: usize,
    deadline: Instant,
) -> io::Result<String> {
    read_line_cancellable(stream, max_bytes, deadline, None)
}

fn read_line_cancellable(
    stream: &mut UnixStream,
    max_bytes: usize,
    deadline: Instant,
    cancelled: Option<&AtomicBool>,
) -> io::Result<String> {
    let mut response = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        stream.set_read_timeout(Some(io_budget(deadline, cancelled)?))?;
        match stream.read(&mut buffer) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "peer closed before completing its response line",
                ));
            }
            Ok(count) => {
                let end = buffer[..count].iter().position(|byte| *byte == b'\n');
                let length = end.map_or(count, |index| index + 1);
                if response.len() + length > max_bytes {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "line too large"));
                }
                response.extend_from_slice(&buffer[..length]);
                if end.is_some() {
                    return String::from_utf8(response)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
                }
            }
            Err(error) if retryable(&error) => continue,
            Err(error) => return Err(error),
        }
    }
}

pub fn write_until(stream: &mut UnixStream, bytes: &[u8], deadline: Instant) -> io::Result<()> {
    write_cancellable(stream, bytes, deadline, None)
}

fn write_cancellable(
    stream: &mut UnixStream,
    mut bytes: &[u8],
    deadline: Instant,
    cancelled: Option<&AtomicBool>,
) -> io::Result<()> {
    while !bytes.is_empty() {
        stream.set_write_timeout(Some(io_budget(deadline, cancelled)?))?;
        match stream.write(bytes) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if retryable(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn io_budget(deadline: Instant, cancelled: Option<&AtomicBool>) -> io::Result<Duration> {
    if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err(io::Error::other("Application launch was cancelled."));
    }
    let remaining = remaining(deadline)?;
    Ok(if cancelled.is_some() {
        remaining.min(Duration::from_millis(20))
    } else {
        remaining
    })
}

fn retryable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "IPC deadline exceeded"))
}

#[cfg(test)]
mod tests;
