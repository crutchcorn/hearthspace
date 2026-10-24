//! Keep native activation alive until GIO receives the Activate or Open reply.
use gio::prelude::*;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

pub const NATIVE_GIO_FLAG: &str = "--launch-native-files";

pub fn native_gio_command(
    desktop_file: &Path,
    paths: &[PathBuf],
    wayland_display: &str,
    environment: &[(&str, &str)],
    helper: &Path,
) -> Result<Command, String> {
    let mut command = crate::app_catalog::launch_command(
        &["/usr/bin/env".into()],
        wayland_display,
        environment,
        None,
    )?;
    command
        .arg(helper)
        .arg(NATIVE_GIO_FLAG)
        .arg(desktop_file)
        .args(paths);
    Ok(command)
}

/// Runs only in the supervised child, under the existing absolute launch budget.
pub fn run_native_gio_helper(mut arguments: impl Iterator<Item = OsString>) -> Result<(), String> {
    let bus = std::env::var_os("HEARTHSPACE_SESSION_BUS_ADDRESS")
        .filter(|address| !address.is_empty())
        .ok_or("The selected desktop session bus is unavailable.")?;
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").as_ref() != Some(&bus) {
        return Err("Native activation helper requires the selected desktop session bus.".into());
    }
    let desktop = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("Missing installed desktop file.")?;
    let app = gio_unix::DesktopAppInfo::from_filename(desktop)
        .ok_or("This application is no longer available.")?;
    let paths: Vec<_> = arguments.map(PathBuf::from).collect();
    if paths.iter().any(|path| !path.is_absolute()) {
        return Err("Select local files or folders to open.".into());
    }
    let uris: Vec<_> = paths
        .iter()
        .map(|path| gio::File::for_path(path).uri())
        .collect();
    let uris: Vec<_> = uris.iter().map(|uri| uri.as_str()).collect();
    let context = gio::AppLaunchContext::new();
    // gio launch uses the synchronous API, which only queues D-Bus Open. Exiting
    // then disconnects the sender; dbus-daemon drops its queued call if the cold
    // service has not acquired its name yet. The async API completes on the reply
    // and also reports activation errors. GIO still owns Exec / URI expansion.
    gio::glib::MainContext::new()
        .block_on(app.launch_uris_future(&uris, Some(&context)))
        .map_err(|error| format!("Could not launch native files: {error}"))
}
