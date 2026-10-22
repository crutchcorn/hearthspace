//! Give native sessions their own desktop bus without modifying the user manager.
use std::{
    env,
    ffi::OsString,
    fs, io,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

use crate::{
    cli::{Args, Backend},
    compositor::{CompositorBackend, select_compositor_backend},
    readiness::remaining,
    session_env::{SessionEnv, prepare_session_directory, runtime_directories},
};

pub(crate) fn bootstrap_native_session(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let native = match args.backend {
        Some(Backend::Native) => true,
        Some(Backend::Nested) => false,
        None => select_compositor_backend() == CompositorBackend::NativeTty,
    };
    if !should_bootstrap(native, args.private_session_bus, args.dry_run) {
        return Ok(());
    }
    let (host, runtime) = runtime_directories(args.session_dir.as_deref())?;
    prepare_session_directory(&runtime, false)?;
    let executable = env::current_exe()?;
    let mut command = bootstrap_command(
        &executable,
        env::args_os().skip(1).collect(),
        &host,
        &runtime,
        args.session_dir.is_some(),
        args.backend.is_some(),
        args.wayland_display
            .as_deref()
            .unwrap_or(hearthspace_common::WAYLAND_DISPLAY_NAME),
    );
    // exec preserves terminal/job-control ownership. dbus-run-session reaps its
    // private daemon when the inner supervisor finishes.
    let error = command.exec();
    let _ = fs::remove_dir(&runtime); // Only our still-empty bootstrap directory.
    Err(format!("could not start private desktop D-Bus session: {error}").into())
}

fn should_bootstrap(native: bool, private_bus: bool, dry_run: bool) -> bool {
    native && !private_bus && !dry_run
}

fn bootstrap_command(
    executable: &Path,
    args: Vec<OsString>,
    host: &Path,
    runtime: &Path,
    has_session_dir: bool,
    has_backend: bool,
    display: &str,
) -> Command {
    let mut command = Command::new("dbus-run-session");
    command
        .arg("--dbus-daemon")
        .arg(executable.with_file_name("hearthspace-session-bus"))
        .arg("--")
        .arg(executable)
        .args(args)
        .arg("--private-session-bus");
    if !has_session_dir {
        command.arg("--session-dir").arg(runtime);
    }
    if !has_backend {
        command.args(["--backend", "native"]);
    }
    command
        .env("XDG_RUNTIME_DIR", runtime)
        .env(hearthspace_common::HOST_RUNTIME_DIR_ENV, host)
        .env("WAYLAND_DISPLAY", display)
        .env("XDG_SESSION_TYPE", "wayland")
        .env("XDG_SESSION_DESKTOP", hearthspace_common::CURRENT_DESKTOP)
        .env("XDG_CURRENT_DESKTOP", hearthspace_common::CURRENT_DESKTOP)
        .env("DESKTOP_SESSION", hearthspace_common::DESKTOP_SESSION);
    for name in [
        "DISPLAY",
        "WAYLAND_SOCKET",
        "DBUS_SESSION_BUS_ADDRESS",
        "DBUS_SESSION_BUS_PID",
        "AT_SPI_BUS_ADDRESS",
    ] {
        command.env_remove(name);
    }
    command
}

/// Update only this explicit private bus. Never import these variables into the
/// shared systemd user manager, which may still own the original desktop and T3.
pub(crate) fn update_activation_environment(
    session: &SessionEnv,
    startup_deadline: Instant,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    let deadline = startup_deadline.min(Instant::now() + Duration::from_secs(2));
    remaining(deadline, cancelled)?;
    let profile = crate::profile_env::for_session(session)?;
    let mut child = activation_command(session, &profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()?;
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "private D-Bus activation environment update failed: {status}"
                )))
            };
        }
        match remaining(deadline, cancelled) {
            Ok(left) => thread::sleep(left.min(Duration::from_millis(5))),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
}

fn activation_command(session: &SessionEnv, app_profile: &[(String, String)]) -> Command {
    let mut command = Command::new("dbus-update-activation-environment");
    command.envs(session.vars.iter().map(|(key, value)| (key, value)));
    for (key, _) in &session.vars {
        command.arg(key);
    }
    // Only activation gets these defaults. The compositor, settings daemon and
    // Settings frontend continue using their authoritative session configuration.
    for (key, value) in app_profile {
        command.env(key, value).arg(key);
    }
    // UpdateActivationEnvironment cannot unset keys. An empty DISPLAY disables
    // host X11 discovery. WAYLAND_SOCKET must be absent, not empty, so the native
    // bootstrap strips it before starting the bus in the first place.
    command.env("DISPLAY", "").arg("DISPLAY");
    if let Some((_, bus)) = session
        .vars
        .iter()
        .find(|(key, _)| key == "HEARTHSPACE_SESSION_BUS_ADDRESS")
    {
        command.env("DBUS_SESSION_BUS_ADDRESS", bus);
    }
    command
}

#[cfg(test)]
mod tests;
