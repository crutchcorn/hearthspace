//! Small external control helpers must not outlive the session's deadline.
use std::{
    io::{self, Read},
    process::{Child, Command, Output, Stdio},
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

use nix::fcntl::{FcntlArg, OFlag, fcntl};

use crate::readiness::remaining;

pub(crate) fn output(
    command: &mut Command,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<Output> {
    remaining(deadline, cancelled)?;
    let mut child = Reap(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let mut stdout = child.0.stdout.take().unwrap();
    let mut stderr = child.0.stderr.take().unwrap();
    fcntl(&stdout, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;
    fcntl(&stderr, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    loop {
        drain(&mut stdout, &mut out)?;
        drain(&mut stderr, &mut err)?;
        if let Some(status) = child.0.try_wait()? {
            drain(&mut stdout, &mut out)?;
            drain(&mut stderr, &mut err)?;
            return Ok(Output {
                status,
                stdout: out,
                stderr: err,
            });
        }
        thread::sleep(remaining(deadline, cancelled)?.min(Duration::from_millis(5)));
    }
}

fn drain(reader: &mut impl Read, output: &mut Vec<u8>) -> io::Result<()> {
    let mut buffer = [0; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => {
                if output.len() + count > 64 * 1024 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "service helper output exceeded 64 KiB",
                    ));
                }
                output.extend_from_slice(&buffer[..count]);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        }
    }
}

struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_helpers_are_killed_and_reaped_at_the_deadline() {
        let start = Instant::now();
        let error = output(
            Command::new("/usr/bin/python3").args(["-c", "import time; time.sleep(60)"]),
            start + Duration::from_millis(50),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn noisy_helpers_cannot_allocate_unbounded_output() {
        let error = output(
            &mut Command::new("/usr/bin/yes"),
            // This exercises child I/O, not helper startup speed under CI load.
            // The byte limit is also checked without scheduling below.
            Instant::now() + Duration::from_secs(5),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn output_limit_includes_bytes_from_previous_drains() {
        let mut output = vec![b'x'; 64 * 1024 - 1];
        drain(&mut io::repeat(b'y').take(1), &mut output).unwrap();
        assert_eq!(output.len(), 64 * 1024);
        assert_eq!(output.last(), Some(&b'y'));

        let error = drain(&mut io::repeat(b'z').take(1), &mut output).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(output.len(), 64 * 1024);
        assert_eq!(output.last(), Some(&b'y'));
    }

    #[test]
    fn unbounded_reader_cannot_retain_more_than_the_output_limit() {
        let mut output = Vec::new();
        let error = drain(&mut io::repeat(b'x'), &mut output).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(output.len(), 64 * 1024);
        assert!(output.iter().all(|&byte| byte == b'x'));
    }
}
