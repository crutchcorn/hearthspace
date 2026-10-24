//! The app-scope stop barrier shared by normal shutdown and login crash cleanup.
//! Call from the cleanup owner, never from the app-scope watchdog being stopped.
use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use super::identity;
use crate::bounded_command;

pub const OWNER_FILE: &str = "app-scope-owner";

/// Require the persisted startup intent, then stop and await every owned scope.
/// No records are removed here: a failed stop remains recoverable.
pub fn shutdown(runtime: &Path, host_runtime: &Path) -> io::Result<()> {
    let Some(recorded) = read_owner(runtime)? else {
        return if scope_records(runtime)?.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other("app scope records have no owner intent"))
        };
    };
    let unit = identity::owner_unit(runtime)?;
    if recorded != unit {
        return Err(io::Error::other(
            "app scope owner intent does not match runtime",
        ));
    }
    let uid = nix::unistd::getuid().as_raw();
    let meta = fs::symlink_metadata(runtime)?;
    if !runtime.is_absolute()
        || !meta.is_dir()
        || meta.uid() != uid
        || meta.mode() & 0o077 != 0
        || runtime.canonicalize()? != runtime
        || !host_runtime.is_absolute()
        || fs::metadata(host_runtime)?.uid() != uid
        || runtime == host_runtime
    {
        return Err(io::Error::other("invalid app scope cleanup runtime"));
    }
    let closing = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(runtime.join(identity::CLOSING_FILE))?;
    if !closing.metadata()?.is_file() {
        return Err(io::Error::other("invalid app scope closing marker"));
    }
    let deadline = Instant::now() + Duration::from_secs(4);
    let cancelled = AtomicBool::new(false);
    let mut dependents = scope_records(runtime)?;
    let owner = properties(host_runtime, &unit, deadline, &cancelled)?;
    if owner.get("LoadState").map(String::as_str) != Some("not-found") {
        if owner.get("Description") != Some(&description(runtime)) {
            return Err(io::Error::other(
                "app scope owner identity changed; refusing stop",
            ));
        }
        let bound: Vec<_> = owner
            .get("BoundBy")
            .into_iter()
            .flat_map(|value| value.split_whitespace())
            .collect();
        if bound.len() > 512 || bound.iter().any(|name| !identity::valid_scope_name(name)) {
            return Err(io::Error::other("unexpected bound app scope identity"));
        }
        dependents.extend(bound.into_iter().map(str::to_owned));
        let mut stop = command(host_runtime, "systemctl");
        stop.args(["--user", "--no-ask-password", "stop", &unit]);
        checked_output(&mut stop, deadline, &cancelled)?;
    }
    dependents.insert(unit);
    loop {
        let mut all_stopped = true;
        for unit in &dependents {
            all_stopped &= stopped(&properties(host_runtime, unit, deadline, &cancelled)?);
        }
        if all_stopped {
            return Ok(());
        }
        std::thread::sleep(
            crate::readiness::remaining(deadline, &cancelled)?.min(Duration::from_millis(50)),
        );
    }
}

fn read_owner(runtime: &Path) -> io::Result<Option<String>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(runtime.join(OWNER_FILE))
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != nix::unistd::getuid().as_raw() {
        return Err(io::Error::other("invalid app scope owner intent"));
    }
    let mut value = String::new();
    file.take(257).read_to_string(&mut value)?;
    if value.len() > 256 {
        return Err(io::Error::other("oversized app scope owner intent"));
    }
    Ok(Some(value))
}

pub fn description(runtime: &Path) -> String {
    format!("Hearthspace app scopes for {}", runtime.display())
}

pub fn command(host_runtime: &Path, program: &str) -> Command {
    let mut command = Command::new(program);
    command.env("XDG_RUNTIME_DIR", host_runtime).env(
        "DBUS_SESSION_BUS_ADDRESS",
        format!("unix:path={}/bus", host_runtime.display()),
    );
    command
}

pub fn properties(
    host_runtime: &Path,
    unit: &str,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<HashMap<String, String>> {
    let mut command = command(host_runtime, "systemctl");
    command.args([
        "--user",
        "--no-ask-password",
        "show",
        unit,
        "--property=LoadState,ActiveState,Description,BoundBy",
    ]);
    // systemctl show may report a not-found unit with a nonzero status.
    let output = bounded_command::output(&mut command, deadline, cancelled)?;
    let properties = parse_properties(&String::from_utf8_lossy(&output.stdout));
    if !output.status.success()
        && properties.get("LoadState").map(String::as_str) != Some("not-found")
    {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(properties)
}

pub fn checked_output(
    command: &mut Command,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let result = bounded_command::output(command, deadline, cancelled)?;
    if !result.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&result.stderr).into_owned(),
        ));
    }
    Ok(())
}

fn parse_properties(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}

fn stopped(properties: &HashMap<String, String>) -> bool {
    properties.get("LoadState").map(String::as_str) == Some("not-found")
        || matches!(
            properties.get("ActiveState").map(String::as_str),
            Some("inactive" | "failed")
        )
}

fn scope_records(runtime: &Path) -> io::Result<BTreeSet<String>> {
    let directory = runtime.join("app-scopes");
    match fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Ok(meta) if meta.is_dir() => {}
        _ => return Err(io::Error::other("invalid app scope record directory")),
    }
    let mut units = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if units.len() >= 512 || !entry.file_type()?.is_file() || !identity::valid_scope_name(&name)
        {
            return Err(io::Error::other("invalid or excessive app scope records"));
        }
        units.insert(name);
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_transitional_properties_never_prove_cleanup() {
        assert!(!stopped(&HashMap::new()));
        assert!(!stopped(&parse_properties("ActiveState=deactivating\n")));
        assert!(stopped(&parse_properties("LoadState=not-found\n")));
        assert!(stopped(&parse_properties("ActiveState=inactive\n")));
    }
}
