//! Keep Ptyxis shell scopes on the host manager and owned by this desktop.
//!
//! Ptyxis 50.1 (and current upstream) invokes the exact prefix below, without
//! checking whether systemd's manager socket exists in its private runtime:
//! https://gitlab.gnome.org/chergert/ptyxis/-/blob/50.1/agent/ptyxis-run-context.c
//! Other systemd-run commands retain their original arguments and environment.
use std::{
    env,
    ffi::{OsStr, OsString},
    fs, io,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::Command,
};

use crate::identity;

const SYSTEMD_RUN: &str = "/usr/bin/systemd-run";
const PREFIX: [&str; 5] = ["--user", "--scope", "--collect", "--quiet", "--same-dir"];

struct Scope<'a> {
    unit: &'a str,
    command_start: usize,
}

pub fn run(args: &[OsString]) -> io::Result<()> {
    let mut command = match scope_invocation(args) {
        Some(scope) => Session::from_environment()?.prepare(args, &scope)?,
        None => {
            let mut command = Command::new(SYSTEMD_RUN);
            command.args(args);
            command
        }
    };
    // Replacing the adapter preserves the PTY, inherited descriptors and
    // original process relationship used by Ptyxis to supervise its shell.
    Err(command.exec())
}

fn scope_invocation(args: &[OsString]) -> Option<Scope<'_>> {
    if !PREFIX
        .iter()
        .enumerate()
        .all(|(index, expected)| args.get(index).is_some_and(|arg| arg == *expected))
    {
        return None;
    }
    let unit = args.get(PREFIX.len())?.to_str()?.strip_prefix("--unit=")?;
    if !unit.starts_with("ptyxis-spawn-") || !identity::valid_scope_name(unit) {
        return None;
    }
    let mut command_start = PREFIX.len() + 1;
    if args.get(command_start).is_some_and(|arg| arg == "--") {
        command_start += 1;
    } else if args.get(command_start)?.as_bytes().starts_with(b"-") {
        return None;
    }
    if args.get(command_start)?.is_empty() {
        return None;
    }
    Some(Scope {
        unit,
        command_start,
    })
}

struct Session {
    runtime: PathBuf,
    host: PathBuf,
    bus: OsString,
    owner: String,
}

impl Session {
    fn from_environment() -> io::Result<Self> {
        let required = |key| {
            env::var_os(key)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| io::Error::other(format!("missing {key} for owned Ptyxis scope")))
        };
        Self::load(
            required("XDG_RUNTIME_DIR")?.into(),
            required(hearthspace_common::HOST_RUNTIME_DIR_ENV)?.into(),
            required("DBUS_SESSION_BUS_ADDRESS")?,
        )
    }

    fn load(runtime: PathBuf, host: PathBuf, bus: OsString) -> io::Result<Self> {
        if !runtime.is_absolute() || !host.is_absolute() || !bus.as_bytes().starts_with(b"unix:") {
            return Err(io::Error::other("invalid private Ptyxis scope environment"));
        }
        owned_directory(&runtime)?;
        owned_directory(&host)?;
        if fs::canonicalize(&runtime)? == fs::canonicalize(&host)? {
            return Err(io::Error::other("Ptyxis scope requires a private runtime"));
        }
        let session = Self {
            owner: identity::owner_unit(&runtime)?,
            runtime,
            host,
            bus,
        };
        session.check_active()?;
        Ok(session)
    }

    fn check_active(&self) -> io::Result<()> {
        // A closing marker also prevents the manager from restarting the owner
        // guard after the final check and before StartTransientUnit completes.
        match fs::symlink_metadata(self.runtime.join(identity::CLOSING_FILE)) {
            Ok(_) => return Err(io::Error::other("desktop session is closing")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        owned_file(&self.runtime.join(identity::READY_FILE))?;
        let owner_path = self.runtime.join("app-scope-owner");
        owned_file(&owner_path)?;
        if fs::read_to_string(owner_path)? != self.owner {
            return Err(io::Error::other(
                "app scope owner does not match private runtime",
            ));
        }
        Ok(())
    }

    fn record(&self, unit: &str) -> io::Result<()> {
        self.check_active()?;
        let records = self.runtime.join("app-scopes");
        match fs::DirBuilder::new().mode(0o700).create(&records) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        owned_directory(&records)?;
        // Register before systemd-run can move the shell out of the login's
        // cgroup. create_new rejects duplicates and existing symlinks.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(records.join(unit))?;
        Ok(())
    }

    fn prepare(&self, args: &[OsString], scope: &Scope<'_>) -> io::Result<Command> {
        self.record(scope.unit)?;
        self.command(args, scope)
    }

    fn command(&self, args: &[OsString], scope: &Scope<'_>) -> io::Result<Command> {
        // Revalidate after recording, covering shutdown/owner changes during
        // preparation. BindsTo+After and the guard cover the remaining exec race.
        self.check_active()?;
        let mut command = Command::new(SYSTEMD_RUN);
        command
            .env("XDG_RUNTIME_DIR", &self.host)
            .env("DBUS_SESSION_BUS_ADDRESS", host_bus(&self.host))
            .args([
                format!("--property=BindsTo={}", self.owner),
                format!("--property=After={}", self.owner),
                "--property=TimeoutStopSec=1".into(),
                "--expand-environment=no".into(),
            ])
            .args(&args[..PREFIX.len() + 1])
            .args(["--", "/usr/bin/env", "--"])
            .arg(assignment("XDG_RUNTIME_DIR", self.runtime.as_os_str()))
            .arg(assignment("DBUS_SESSION_BUS_ADDRESS", &self.bus))
            .args(&args[scope.command_start..]);
        Ok(command)
    }
}

fn owned_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::other(
            "scope runtime must be an owned private directory",
        ));
    }
    Ok(())
}

fn owned_file(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.uid() != nix::unistd::geteuid().as_raw() {
        return Err(io::Error::other(
            "scope ownership record must be an owned regular file",
        ));
    }
    Ok(())
}

fn assignment(key: &str, value: &OsStr) -> OsString {
    let mut result = OsString::from(key);
    result.push("=");
    result.push(value);
    result
}

fn host_bus(host: &Path) -> OsString {
    // D-Bus addresses require percent-encoding path bytes outside this set.
    let path = host.join("bus");
    let mut address = String::from("unix:path=");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"_-/.\\".contains(&byte) {
            address.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(address, "%{byte:02X}").unwrap();
        }
    }
    address.into()
}

#[cfg(test)]
#[path = "ptyxis_run/tests.rs"]
mod tests;
