//! Keep the private desktop bus reachable across confined apps' mount namespaces.
use std::{env, io, os::unix::process::CommandExt, process::Command};

#[path = "../private_bus_address.rs"]
mod private_bus_address;

fn main() -> io::Result<()> {
    let address = private_bus_address::generate()?;
    // Keep the standard session policy, authentication and inherited readiness
    // descriptors. Only its listen address changes: Snap's private /tmp hides
    // filesystem sockets, while its desktop policy permits @/tmp/dbus-*.
    let error = Command::new("/usr/bin/dbus-daemon")
        .args(env::args_os().skip(1))
        .arg(format!("--address={address}"))
        .exec();
    Err(error)
}
