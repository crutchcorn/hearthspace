//! A confined application's cold start must inherit its prepared scope and env.
//! Native handlers retain normal GIO activation; scoped handlers use GIO's Exec
//! fallback in this helper process, with the real private bus in the child context.
use gio::prelude::*;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

pub const SCOPED_GIO_FLAG: &str = "--launch-scoped-files";
// /dev/null cannot be a D-Bus socket. An explicit unavailable address prevents
// autolaunch and makes GIO use Exec without involving a broker outside our scope.
const NO_ACTIVATION_BUS: &str = "unix:path=/dev/null";

pub fn scoped_gio_command(
    desktop_file: &Path,
    paths: &[PathBuf],
    prefix: &[String],
    wayland_display: &str,
    environment: &[(&str, &str)],
    helper: &Path,
) -> Result<Command, String> {
    let mut argv = prefix.to_vec();
    argv.extend([
        "/usr/bin/env".into(),
        format!("DBUS_SESSION_BUS_ADDRESS={NO_ACTIVATION_BUS}"),
    ]);
    let mut command =
        crate::app_catalog::launch_command(&argv, wayland_display, environment, None)?;
    command
        .arg(helper)
        .arg(SCOPED_GIO_FLAG)
        .arg(desktop_file)
        .args(paths);
    Ok(command)
}

/// Called before normal argument parsing / startup in a dedicated child only.
/// Never mutate the shell or compositor process environment to select a backend.
pub fn run_scoped_gio_helper(mut arguments: impl Iterator<Item = OsString>) -> Result<(), String> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").as_deref()
        != Some(std::ffi::OsStr::new(NO_ACTIVATION_BUS))
    {
        return Err("Scoped file helper requires its activation-disabled environment.".into());
    }
    let bus = std::env::var_os("HEARTHSPACE_SESSION_BUS_ADDRESS")
        .filter(|address| !address.is_empty())
        .ok_or("The private desktop session bus is unavailable.")?;
    let desktop = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("Missing installed desktop file.")?;
    let app = gio_unix::DesktopAppInfo::from_filename(&desktop)
        .ok_or("This application is no longer available.")?;
    if app.commandline().is_none() {
        return Err("This scoped application has no Exec launch command.".into());
    }
    let paths: Vec<_> = arguments.map(PathBuf::from).collect();
    if paths.iter().any(|path| !path.is_absolute()) {
        return Err("Select local files or folders to open.".into());
    }
    let context = gio::AppLaunchContext::new();
    context.setenv("DBUS_SESSION_BUS_ADDRESS", bus);
    context.unsetenv("DBUS_SESSION_BUS_PID");
    let files: Vec<_> = paths.iter().map(gio::File::for_path).collect();
    // Load the original filename, not a modified desktop copy: GIO retains %k,
    // localized metadata, working-directory rules, and native-byte %f/%U expansion.
    app.launch(&files, Some(&context))
        .map_err(|error| format!("Could not launch scoped files: {error}"))
}
