//! One address usable by zbus and by apps with a private /tmp mount.
use std::{fs, io};

pub(crate) fn generate() -> io::Result<String> {
    let uuid = fs::read_to_string("/proc/sys/kernel/random/uuid")?;
    Ok(format!(
        "unix:abstract=/tmp/dbus-hearthspace-{}",
        uuid.trim()
    ))
}
