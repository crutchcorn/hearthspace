//! Opt-in launch lifetime: monotonic deadline plus a write side kept open until
//! acceptance. EOF revokes this request; legacy line requests still half-close.
use super::*;
use crate::command::ShellCommand;

mod cancellation;
pub use cancellation::Cancellation;

const PREFIX: &str = "launch-request-v1 ";
// A caller can shorten the compositor's preparation budget, never extend it.
const MAX_BUDGET: Duration = Duration::from_secs(8);

#[derive(Debug)]
pub struct Envelope {
    pub command: String,
    pub deadline: Instant,
}

impl Envelope {
    pub fn encode(command: &str, deadline: Instant) -> io::Result<String> {
        validate_command(command)?;
        let ticks = monotonic_nanos()?;
        let remaining = remaining(deadline)?.min(MAX_BUDGET);
        let end = ticks
            .checked_add(remaining.as_nanos() as u64)
            .ok_or_else(|| io::Error::other("launch deadline overflow"))?;
        let line = format!("{PREFIX}{end} {command}\n");
        if line.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "launch request too large",
            ));
        }
        Ok(line)
    }

    /// Return None for the legacy protocol. Never extend a deadline on decode,
    /// even if the client spent most of its budget queued before connecting.
    pub fn decode(line: &[u8]) -> io::Result<Option<Self>> {
        if !line.starts_with(PREFIX.as_bytes()) {
            return Ok(None);
        }
        if line.len() > MAX_REQUEST_BYTES || !line.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid launch envelope",
            ));
        }
        let text = std::str::from_utf8(line).map_err(io::Error::other)?;
        let (deadline, command) = text[PREFIX.len()..text.len() - 1]
            .split_once(' ')
            .ok_or_else(|| io::Error::other("invalid launch envelope"))?;
        validate_command(command)?;
        let deadline = deadline.parse::<u64>().map_err(io::Error::other)?;
        let now = Instant::now();
        let remaining = deadline
            .checked_sub(monotonic_nanos()?)
            .filter(|remaining| *remaining != 0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "launch deadline expired"))?;
        Ok(Some(Self {
            command: command.into(),
            deadline: now + Duration::from_nanos(remaining).min(MAX_BUDGET),
        }))
    }
}

pub fn request(
    path: &Path,
    command: &str,
    deadline: Instant,
    cancellation: &Cancellation,
) -> io::Result<String> {
    let cancelled = &cancellation.cancelled;
    io_budget(deadline, Some(cancelled))?;
    let line = Envelope::encode(command, deadline)?;
    let mut stream = connect_until(path, deadline)?;
    let _registration = cancellation.register(&stream)?;
    // Do not half-close: cancellation/deadline drops the socket, which the
    // compositor can observe independently of its event-loop dispatch order.
    let result = write_cancellable(&mut stream, line.as_bytes(), deadline, Some(cancelled))
        .and_then(|()| {
            read_line_cancellable(&mut stream, MAX_RESPONSE_BYTES, deadline, Some(cancelled))
        });
    if cancellation.is_cancelled() {
        Err(cancellation.error())
    } else {
        result
    }
}

fn validate_command(command: &str) -> io::Result<()> {
    let allowed = matches!(
        ShellCommand::parse(command),
        Some(
            ShellCommand::LaunchApp(_)
                | ShellCommand::LaunchAppOnOutput { .. }
                | ShellCommand::OpenSearchSettings { .. }
        )
    ) || [
        "favorite-gesture-launch ",
        "desktop-context-background ",
        "launch-files ",
    ]
    .iter()
    .any(|prefix| command.starts_with(prefix));
    if !allowed || command.contains(['\r', '\n']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "envelope requires one launch command",
        ));
    }
    Ok(())
}

fn monotonic_nanos() -> io::Result<u64> {
    let mut now = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes one timespec through a valid pointer. The
    // monotonic clock is shared by these local processes and ignores wall time.
    if unsafe { nix::libc::clock_gettime(nix::libc::CLOCK_MONOTONIC, &mut now) } != 0 {
        return Err(io::Error::last_os_error());
    }
    u64::try_from(now.tv_sec)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000_000_000))
        .and_then(|seconds| seconds.checked_add(now.tv_nsec as u64))
        .ok_or_else(|| io::Error::other("invalid monotonic clock"))
}

#[cfg(test)]
mod tests;
