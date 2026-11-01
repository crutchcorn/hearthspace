//! Own the attachment fixture's desktop and bus groups, including activation
//! children. This covers children that inherit the group, not daemon escape.
use std::{
    fs,
    io::{self, Write},
    os::unix::process::CommandExt,
    process::{Child, ChildStdout, Command},
    thread,
    time::{Duration, Instant},
};

use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};

const POLL: Duration = Duration::from_millis(10);
const TERM_BUDGET: Duration = Duration::from_secs(2);
const KILL_BUDGET: Duration = Duration::from_secs(2);

enum Census {
    Complete(Vec<Process>),
    // The caller's phase budget expired; partial observations prove nothing.
    DeadlineReached,
}

enum QuiescenceError {
    Ownership(io::Error),
    Census(io::Error),
}

pub(super) struct OwnedProcess {
    // Never expose this handle: reaping the leader releases the PID that pins
    // our group identity, even when descendants are still running.
    child: Option<Child>,
    pid: Pid,
    start_ticks: Option<u64>,
}

impl OwnedProcess {
    pub(super) fn spawn(command: &mut Command) -> io::Result<Self> {
        let child = command.process_group(0).spawn()?;
        let mut owned = Self {
            pid: Pid::from_child(&child),
            child: Some(child),
            start_ticks: None,
        };
        // Install the guard before any fallible inspection. Even if this read
        // fails, WNOWAIT can still prove ownership of our unreaped direct child.
        owned.start_ticks = Some(owned.leader()?.start_ticks);
        Ok(owned)
    }

    pub(super) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }

    pub(super) fn is_running(&self) -> io::Result<bool> {
        if self.child.is_none() {
            return Ok(false);
        }
        let exited = self.exited()?;
        self.leader()?;
        Ok(!exited)
    }

    fn exited(&self) -> io::Result<bool> {
        // A specific child only; never consume another fixture's exit status.
        waitid(
            WaitId::Pid(self.pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .map(|status| status.is_some())
        .map_err(Into::into)
    }

    fn leader(&self) -> io::Result<Process> {
        let pid = self.pid.as_raw_pid() as u32;
        let leader = read_process(pid)?.ok_or_else(|| {
            io::Error::other(format!(
                "owned leader {pid} disappeared before it was reaped"
            ))
        })?;
        if pid <= 1
            || leader.group != pid
            || self
                .start_ticks
                .is_some_and(|start| start != leader.start_ticks)
        {
            return Err(io::Error::other(format!(
                "owned leader identity changed: {leader:?}"
            )));
        }
        Ok(leader)
    }

    fn signal(&self, signal: Signal) -> io::Result<()> {
        self.exited()?; // ECHILD is an ownership failure, not successful cleanup.
        self.leader()?;
        match kill_process_group(self.pid, signal) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn quiescent(
        &self,
        deadline: Instant,
        census: &mut impl FnMut(Pid, Instant) -> io::Result<Census>,
    ) -> Result<bool, QuiescenceError> {
        let mut empty_once = false;
        loop {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            let exited = self.exited().map_err(QuiescenceError::Ownership)?;
            self.leader().map_err(QuiescenceError::Ownership)?;
            let members = match census(self.pid, deadline).map_err(QuiescenceError::Census)? {
                Census::Complete(members) => members,
                Census::DeadlineReached => return Ok(false),
            };
            // A zombie main thread can still have live workers. Fully dead
            // descendants have another adopter; only the leader is ours to reap.
            let empty = exited && members.iter().all(|member| !member.live());
            if empty && empty_once {
                return Ok(true);
            }
            empty_once = empty;
            thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }

    pub(super) fn stop(&mut self) -> io::Result<()> {
        self.stop_with_census(&mut group_members)
    }

    fn stop_with_census(
        &mut self,
        census: &mut impl FnMut(Pid, Instant) -> io::Result<Census>,
    ) -> io::Result<()> {
        if self.child.is_none() {
            return Ok(());
        }
        let mut census_error = None;
        for (signal, budget) in [(Signal::TERM, TERM_BUDGET), (Signal::KILL, KILL_BUDGET)] {
            // A failed census must not abandon resistant children. Every
            // escalation independently revalidates the still-unreaped leader;
            // lost ownership, unlike a census failure, forbids group signaling.
            self.signal(signal)
                .map_err(|error| cleanup_error(census_error.take(), error))?;
            match self.quiescent(Instant::now() + budget, census) {
                Ok(true) => {
                    // WNOWAIT already observed terminal status. Never reap the
                    // anchor while census failure leaves quiescence unverified.
                    self.child
                        .as_mut()
                        .unwrap()
                        .wait()
                        .map_err(|error| cleanup_error(census_error.take(), error))?;
                    self.child = None;
                    return census_error.map_or(Ok(()), Err);
                }
                Ok(false) => {}
                Err(QuiescenceError::Census(error)) => {
                    census_error.get_or_insert(error);
                }
                Err(QuiescenceError::Ownership(error)) => {
                    return Err(cleanup_error(census_error, error));
                }
            }
        }
        Err(census_error.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "could not verify quiescence of owned process group {} after TERM and KILL",
                    self.pid
                ),
            )
        }))
    }
}

fn cleanup_error(first: Option<io::Error>, next: io::Error) -> io::Error {
    match first {
        Some(first) => io::Error::new(
            first.kind(),
            format!("{first}; cleanup also failed: {next}"),
        ),
        None => next,
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            if thread::panicking() {
                // Broken stderr must not cause a second panic during cleanup.
                let _ = writeln!(
                    io::stderr().lock(),
                    "fixture group {} cleanup: {error}",
                    self.pid
                );
            } else {
                panic!("fixture group {} cleanup: {error}", self.pid);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Process {
    pid: u32,
    group: u32,
    start_ticks: u64,
    state: u8,
    threads: u32,
}

impl Process {
    fn live(self) -> bool {
        self.threads > 1 || !matches!(self.state, b'Z' | b'X')
    }
}

fn read_process(pid: u32) -> io::Result<Option<Process>> {
    let bytes = match fs::read(format!("/proc/{pid}/stat")) {
        Ok(bytes) => bytes,
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(rustix::io::Errno::SRCH.raw_os_error()) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    // comm may contain spaces, closing parentheses, or non-UTF-8 bytes.
    let invalid = || io::Error::other(format!("invalid /proc/{pid}/stat"));
    let end = bytes
        .iter()
        .rposition(|byte| *byte == b')')
        .ok_or_else(invalid)?;
    let fields: Vec<_> = std::str::from_utf8(&bytes[end + 1..])
        .map_err(|_| invalid())?
        .split_whitespace()
        .collect();
    if fields.len() < 20 || fields[0].len() != 1 {
        return Err(invalid());
    }
    Ok(Some(Process {
        pid,
        state: fields[0].as_bytes()[0],
        group: fields[2].parse().map_err(|_| invalid())?,
        threads: fields[17].parse().map_err(|_| invalid())?,
        start_ticks: fields[19].parse().map_err(|_| invalid())?,
    }))
}

fn group_members(group: Pid, deadline: Instant) -> io::Result<Census> {
    let mut members = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        if Instant::now() >= deadline {
            return Ok(Census::DeadlineReached);
        }
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
        else {
            continue;
        };
        if let Some(process) = read_process(pid)?
            && process.group == group.as_raw_pid() as u32
        {
            members.push(process);
        }
    }
    // Even the last read may cross the deadline. Only a complete scan within
    // the phase budget can establish quiescence. Actual I/O/parse errors above
    // remain errors, including I/O errors whose kind happens to be TimedOut.
    if Instant::now() >= deadline {
        Ok(Census::DeadlineReached)
    } else {
        Ok(Census::Complete(members))
    }
}

#[cfg(test)]
#[path = "owned_process/tests.rs"]
mod tests;
