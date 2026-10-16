//! Bounded initial readiness, separate from post-startup interaction deadlines.
use std::{
    io,
    os::unix::net::UnixStream,
    path::Path,
    process::{Child, ExitStatus},
    thread,
    time::{Duration, Instant},
};

// A cold software EGL/LLVM initialization can exceed ten seconds on CI. Give
// that one startup phase more room without restarting a failed compositor or
// extending the existing screenshot, accessibility, and shutdown deadlines.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(super) fn wait_for_socket(child: &mut Child, path: &Path) -> io::Result<UnixStream> {
    let started = Instant::now();
    wait_for_socket_with(
        STARTUP_TIMEOUT,
        || started.elapsed(),
        || child.try_wait(),
        || UnixStream::connect(path),
        thread::sleep,
    )
}

fn wait_for_socket_with(
    timeout: Duration,
    elapsed: impl Fn() -> Duration,
    mut child_status: impl FnMut() -> io::Result<Option<ExitStatus>>,
    mut connect: impl FnMut() -> io::Result<UnixStream>,
    mut pause: impl FnMut(Duration),
) -> io::Result<UnixStream> {
    let mut last_error = None;
    loop {
        if let Some(status) = child_status()? {
            return Err(io::Error::other(format!(
                "compositor exited before accepting commands after {:?}: {status}",
                elapsed()
            )));
        }
        let remaining = timeout.saturating_sub(elapsed());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "compositor did not accept commands within {timeout:?}; last connection error: {last_error:?}"
                ),
            ));
        }
        match connect() {
            Ok(stream) => return Ok(stream),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
        // A slow probe also consumes the same absolute readiness allowance.
        pause(POLL_INTERVAL.min(timeout.saturating_sub(elapsed())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, os::unix::process::ExitStatusExt};

    #[test]
    fn cold_start_can_become_ready_after_the_previous_ten_second_limit() {
        let elapsed = Cell::new(Duration::ZERO);
        let ready_at = Duration::from_secs(12);
        let (stream, _peer) = UnixStream::pair().unwrap();
        let mut stream = Some(stream);
        let result = wait_for_socket_with(
            STARTUP_TIMEOUT,
            || elapsed.get(),
            || Ok(None),
            || {
                if elapsed.get() >= ready_at {
                    Ok(stream.take().unwrap())
                } else {
                    Err(io::Error::from(io::ErrorKind::NotFound))
                }
            },
            |duration| elapsed.set(elapsed.get() + duration),
        );
        assert!(result.is_ok());
        assert_eq!(elapsed.get(), ready_at);
    }

    #[test]
    fn absent_socket_exhausts_one_budget_without_connecting_after_the_deadline() {
        let elapsed = Cell::new(Duration::ZERO);
        let attempts = Cell::new(0);
        let timeout = Duration::from_millis(125);
        let error = wait_for_socket_with(
            timeout,
            || elapsed.get(),
            || Ok(None),
            || {
                assert!(elapsed.get() < timeout);
                attempts.set(attempts.get() + 1);
                Err(io::Error::from(io::ErrorKind::ConnectionRefused))
            },
            |duration| elapsed.set(elapsed.get() + duration),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("ConnectionRefused"));
        assert_eq!(elapsed.get(), timeout);
        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn exited_child_fails_on_the_next_poll_without_another_connection_attempt() {
        let elapsed = Cell::new(Duration::ZERO);
        let attempts = Cell::new(0);
        let error = wait_for_socket_with(
            STARTUP_TIMEOUT,
            || elapsed.get(),
            || Ok((!elapsed.get().is_zero()).then(|| ExitStatus::from_raw(23 << 8))),
            || {
                attempts.set(attempts.get() + 1);
                Err(io::Error::from(io::ErrorKind::NotFound))
            },
            |duration| elapsed.set(elapsed.get() + duration),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exited before accepting commands")
        );
        assert!(error.to_string().contains("23"));
        assert_eq!(elapsed.get(), POLL_INTERVAL);
        assert_eq!(attempts.get(), 1);
    }

    #[test]
    fn unexpected_connection_error_is_not_retried() {
        let error = wait_for_socket_with(
            STARTUP_TIMEOUT,
            || Duration::ZERO,
            || Ok(None),
            || Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            |_| panic!("permanent connection errors must not delay failure"),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn process_poll_error_fails_before_connecting() {
        let error = wait_for_socket_with(
            STARTUP_TIMEOUT,
            || Duration::ZERO,
            || Err(io::Error::other("could not poll child")),
            || panic!("must establish child liveness before probing readiness"),
            |_| panic!("failed process polls must not delay failure"),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "could not poll child");
    }
}
