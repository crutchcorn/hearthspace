//! Retain the private process-group leader until terminal helper teardown.
use std::{
    io,
    os::fd::{AsFd, OwnedFd},
    process::{Child, ChildStdout, Command},
    thread,
    time::{Duration, Instant},
};

use rustix::process::{
    Pid, PidfdFlags, Signal, WaitId, WaitIdOptions, getpgid, kill_process_group, pidfd_open, waitid,
};
use tracing::warn;

pub(in crate::compositor) struct Satellite {
    child: Child,
    pidfd: OwnedFd,
}

impl Satellite {
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        let mut child = command.spawn()?;
        let pid = Pid::from_raw(child.id() as i32).expect("child PID is nonzero");
        match pidfd_open(pid, PidfdFlags::empty()) {
            Ok(pidfd) => Ok(Self { child, pidfd }),
            Err(error) => {
                // This newly spawned direct child has never been reaped. Keep
                // its anchor until its private group has received the signal.
                let _ = kill_process_group(pid, Signal::KILL);
                let _ = child.wait();
                Err(error.into())
            }
        }
    }

    pub(super) fn take_stdout(&mut self) -> ChildStdout {
        self.child.stdout.take().expect("Satellite stdout is piped")
    }
}

impl Drop for Satellite {
    fn drop(&mut self) {
        // No caller polls/reaps Child. The pidfd additionally rejects an
        // externally reaped leader; an exited but unreaped leader pins PGID.
        let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
        let pid = Pid::from_raw(self.child.id() as i32).expect("child PID is nonzero");
        if waitid(WaitId::PidFd(self.pidfd.as_fd()), options).is_err()
            || getpgid(Some(pid)).ok() != Some(pid)
        {
            warn!(
                pid = self.child.id(),
                "lost Satellite group ownership; recovery required"
            );
            return;
        }
        if let Err(error) = kill_process_group(pid, Signal::KILL) {
            warn!(%error, "could not stop Satellite group; recovery required");
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut empty_once = false;
        loop {
            match waitid(WaitId::PidFd(self.pidfd.as_fd()), options) {
                Ok(Some(_)) => match group_is_live(pid, deadline) {
                    Ok(false) if empty_once => {
                        if let Err(error) = self.child.wait() {
                            warn!(%error, "could not reap Satellite");
                        }
                        return;
                    }
                    Ok(live) => empty_once = !live,
                    Err(error) => {
                        warn!(%error, "Satellite group census failed; recovery required");
                        let _ = self.child.wait();
                        return;
                    }
                },
                Ok(None) if Instant::now() < deadline => {}
                result => {
                    warn!(?result, "Satellite did not retire; recovery required");
                    return;
                }
            }
            if Instant::now() >= deadline {
                warn!("Satellite group did not retire; recovery required");
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn group_is_live(pid: Pid, deadline: Instant) -> io::Result<bool> {
    for entry in std::fs::read_dir("/proc")? {
        if Instant::now() >= deadline {
            return Ok(true); // An incomplete census never establishes quiescence.
        }
        let entry = entry?;
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if matches!(error.kind(), io::ErrorKind::NotFound) => continue,
            Err(error) => return Err(error),
        };
        let fields = stat
            .rsplit_once(')')
            .ok_or_else(|| io::Error::other("invalid process stat"))?
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        let group = fields
            .get(2)
            .and_then(|field| field.parse::<i32>().ok())
            .ok_or_else(|| io::Error::other("invalid process group"))?;
        let threads = fields
            .get(17)
            .and_then(|field| field.parse::<u32>().ok())
            .ok_or_else(|| io::Error::other("invalid process thread count"))?;
        let state = fields
            .first()
            .ok_or_else(|| io::Error::other("invalid process state"))?;
        if group == pid.as_raw_nonzero().get() && (!matches!(*state, "Z" | "X") || threads > 1) {
            return Ok(true);
        }
    }
    Ok(Instant::now() >= deadline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::BufRead, os::unix::process::CommandExt, process::Stdio};

    fn helper(script: &str) -> Satellite {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", script])
            .process_group(0)
            .stdout(Stdio::piped());
        Satellite::spawn(&mut command).unwrap()
    }

    #[test]
    fn teardown_reaps_helper_without_signaling_an_unrelated_group() {
        let sentinel = helper("exec sleep 30");
        let owned = helper("exec sleep 30");
        let pid = Pid::from_raw(owned.child.id() as i32).unwrap();
        drop(owned);
        assert!(
            matches!(
                waitid(
                    WaitId::Pid(pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG
                ),
                Err(rustix::io::Errno::CHILD)
            ),
            "teardown must reap its direct child"
        );
        assert!(matches!(
            waitid(
                WaitId::PidFd(sentinel.pidfd.as_fd()),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT
            ),
            Ok(None)
        ));
    }

    #[test]
    fn exited_helper_keeps_its_group_owned_until_descendants_stop() {
        let mut owned = helper("sleep 30 & printf '%s\\n' \"$!\"; exit 0");
        let mut line = String::new();
        std::io::BufReader::new(owned.take_stdout())
            .read_line(&mut line)
            .unwrap();
        let descendant = Pid::from_raw(line.trim().parse().unwrap()).unwrap();
        let group = Pid::from_raw(owned.child.id() as i32).unwrap();
        assert_eq!(getpgid(Some(descendant)).unwrap(), group);
        let deadline = Instant::now() + Duration::from_secs(2);
        while waitid(
            WaitId::PidFd(owned.pidfd.as_fd()),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .unwrap()
        .is_none()
        {
            assert!(Instant::now() < deadline, "helper leader did not exit");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(group_is_live(group, deadline).unwrap());
        drop(owned);
        assert!(!group_is_live(group, Instant::now() + Duration::from_secs(2)).unwrap());
        assert!(matches!(
            waitid(
                WaitId::Pid(group),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG
            ),
            Err(rustix::io::Errno::CHILD)
        ));
    }
}
