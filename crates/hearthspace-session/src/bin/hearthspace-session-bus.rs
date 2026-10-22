//! Keep the private desktop bus reachable across confined apps' mount namespaces.
use std::{env, fs, io, os::unix::process::CommandExt, process::Command};

fn main() -> io::Result<()> {
    let uuid = fs::read_to_string("/proc/sys/kernel/random/uuid")?;
    // Keep the standard session policy, authentication and inherited readiness
    // descriptors. Only its listen address changes: Snap's private /tmp hides
    // filesystem sockets, while its desktop policy permits @/tmp/dbus-*.
    let error = Command::new("/usr/bin/dbus-daemon")
        .args(env::args_os().skip(1))
        .arg(format!(
            "--address=unix:abstract=/tmp/dbus-hearthspace-{}",
            uuid.trim()
        ))
        .exec();
    Err(error)
}
