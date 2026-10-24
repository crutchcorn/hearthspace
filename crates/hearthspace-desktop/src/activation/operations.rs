//! Deadline-bounded helpers shared by publication and crash retirement.
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    future::Future,
    io,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use nix::{
    errno::Errno,
    fcntl::{Flock, FlockArg, OFlag},
};

pub const TIMEOUT: Duration = Duration::from_secs(2);

pub fn remaining(deadline: Instant, cancelled: &AtomicBool) -> io::Result<Duration> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "activation cancelled",
        ));
    }
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "activation deadline exceeded"))
}

pub fn lock(host: &Path, deadline: Instant, cancelled: &AtomicBool) -> io::Result<Flock<File>> {
    // Never unlink: waiting processes must keep sharing the same lock inode.
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK).bits())
        .open(host.join("hearthspace-activation.lock"))?;
    if !super::journal::owned_file(&file.metadata()?) {
        return Err(io::Error::other("invalid activation lock file"));
    }
    loop {
        remaining(deadline, cancelled)?;
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => return Ok(lock),
            Err((returned, Errno::EAGAIN)) => file = returned,
            Err((_, error)) => return Err(error.into()),
        }
        thread::sleep(remaining(deadline, cancelled)?.min(Duration::from_millis(5)));
    }
}

pub fn bounded<T>(
    deadline: Instant,
    cancelled: &AtomicBool,
    future: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    remaining(deadline, cancelled)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::pin!(future);
            loop {
                let left = remaining(deadline, cancelled)?;
                tokio::select! {
                    result = &mut future => return result,
                    _ = tokio::time::sleep(left.min(Duration::from_millis(5))) => {}
                }
            }
        })
}

pub fn update(
    bus: &str,
    values: &BTreeMap<String, String>,
    systemd: bool,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    remaining(deadline, cancelled)?;
    let mut child = Reap(command(bus, values, systemd).spawn()?);
    loop {
        if let Some(status) = child.0.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("activation environment update failed"))
            };
        }
        thread::sleep(remaining(deadline, cancelled)?.min(Duration::from_millis(5)));
    }
}

pub fn command(bus: &str, values: &BTreeMap<String, String>, systemd: bool) -> Command {
    let mut command = Command::new("/usr/bin/dbus-update-activation-environment");
    command
        .env_clear()
        .env("DBUS_SESSION_BUS_ADDRESS", bus)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if systemd {
        // The updater checks its own runtime's systemd directory before honoring
        // --systemd; the VAR=value argument alone does not configure that check.
        if let Some(runtime) = values.get("XDG_RUNTIME_DIR") {
            command.env("XDG_RUNTIME_DIR", runtime);
        }
        command.arg("--systemd");
    }
    for (key, value) in values {
        command.arg(format!("{key}={value}"));
    }
    command
}

struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
