//! Tie a complete graphical login to its supervisor, including crash cleanup.
use std::{
    env, fs, io,
    process::Command,
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

use crate::{
    bounded_command,
    cli::{Args, Backend},
    readiness::remaining,
    session_env::SessionPaths,
};

pub(crate) fn validate(args: &Args) -> io::Result<()> {
    if !args.login_session {
        return Ok(());
    }
    if !args.installed || !matches!(args.backend, Some(Backend::Native)) {
        return Err(io::Error::other(
            "--login-session requires --installed --backend native",
        ));
    }
    // A terminal inside an existing desktop must not turn that entire login
    // into a disposable session. The isolated native harness supplies its own display.
    if !args.private_session_bus
        && ["WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY"]
            .iter()
            .any(|key| env::var_os(key).is_some_and(|value| !value.is_empty()))
    {
        return Err(io::Error::other(
            "start --login-session from the display manager, not inside an existing desktop",
        ));
    }
    Ok(())
}

pub(crate) fn start_guard(
    args: &Args,
    paths: &SessionPaths,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let executable = env::current_exe()?;
    let helper = args
        .binary_dir
        .as_deref()
        .unwrap_or_else(|| executable.parent().expect("executable directory"))
        .join("hearthspace-login-guard");
    if !helper.is_file() {
        return Err(io::Error::other(format!(
            "missing installed login cleanup helper: {}",
            helper.display()
        )));
    }
    let pid = std::process::id();
    let stat = fs::read_to_string("/proc/self/stat")?;
    let start = stat
        .rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .ok_or_else(|| io::Error::other("could not identify login supervisor"))?;
    let unit = format!("hearthspace-login-{pid}-{start}.service");
    let mut command = Command::new("systemd-run");
    command
        .env("XDG_RUNTIME_DIR", &paths.host_runtime_dir)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/bus", paths.host_runtime_dir.display()),
        )
        .args([
            "--user",
            "--collect",
            "--quiet",
            "--no-ask-password",
            "--service-type=exec",
            "--property=Restart=no",
            "--property=TimeoutStopSec=5",
            "--unit",
            &unit,
        ])
        .arg(helper)
        .arg(pid.to_string())
        .arg(start)
        .arg(&paths.session_dir);
    let output = bounded_command::output(&mut command, deadline, cancelled)?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "login cleanup guard failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let ready = paths.session_dir.join("login-guard-ready.json");
    while !ready.is_file() {
        thread::sleep(remaining(deadline, cancelled)?.min(Duration::from_millis(20)));
    }
    tracing::info!(%unit, "display-manager login cleanup guard ready");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn login_ownership_is_explicit_and_never_available_in_nested_mode() {
        assert!(Args::try_parse_from(["session", "--login-session"]).is_err());
        let args = Args::parse_from([
            "session",
            "--login-session",
            "--installed",
            "--backend",
            "nested",
        ]);
        assert!(validate(&args).is_err());
        let args = Args::parse_from(["session", "--installed", "--backend", "nested"]);
        validate(&args).unwrap();
    }
}
