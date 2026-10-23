//! GIO selects installed handlers; the compositor owns their launch environment.
use gio::prelude::*;
use hearthspace_common::desktop_files::{file_uri, local_path};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

mod process;
pub use process::{run_opener, run_opener_until};
mod scoped;
pub use scoped::{SCOPED_GIO_FLAG, run_scoped_gio_helper, scoped_gio_command};
mod native;
pub use native::{NATIVE_GIO_FLAG, native_gio_command, run_native_gio_helper};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub app_id: String,
    pub uris: Vec<String>,
    pub output_name: Option<String>,
}

impl Request {
    pub fn for_default(path: &Path, output: Option<&str>) -> Result<Self, String> {
        validate_paths(&[path.to_owned()])?;
        let handler = gio::File::for_path(path)
            .query_default_handler(gio::Cancellable::NONE)
            .map_err(|error| format!("Could not find an application to open this file: {error}"))?;
        let id = handler
            .id()
            .ok_or("The associated application has no installed desktop entry.")?;
        Self::for_app(&[path.to_owned()], &id, output)
    }

    pub fn for_app(paths: &[PathBuf], app_id: &str, output: Option<&str>) -> Result<Self, String> {
        validate_paths(paths)?;
        installed_application(app_id)?;
        Ok(Self {
            app_id: app_id.into(),
            uris: paths
                .iter()
                .map(|path| file_uri(path))
                .collect::<Result<_, _>>()?,
            output_name: output.map(str::to_owned),
        })
    }

    pub fn paths(&self) -> Result<Vec<PathBuf>, String> {
        let paths = self
            .uris
            .iter()
            .map(|uri| local_path(uri))
            .collect::<Result<Vec<_>, _>>()?;
        validate_paths(&paths)?;
        Ok(paths)
    }

    pub fn send(&self, socket: &Path) -> Result<(), String> {
        let command = format!(
            "launch-files {}\n",
            serde_json::to_string(self).map_err(|e| e.to_string())?
        );
        // Longer than the bounded GIO wait, shorter than the compositor's ten-second deadline.
        let response =
            hearthspace_ipc::transport::request_line(socket, &command, Duration::from_secs(9))
                .map_err(|error| format!("Could not confirm file launch: {error}"))?;
        match response.trim_end() {
            "ok" => Ok(()),
            response => Err(response
                .strip_prefix("err ")
                .unwrap_or("Invalid compositor response")
                .into()),
        }
    }
}

fn validate_paths(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() || paths.len() > hearthspace_common::MAX_DESKTOP_ITEMS {
        return Err("Select local files or folders to open.".into());
    }
    if paths
        .iter()
        .any(|path| !path.is_absolute() || !path.exists())
    {
        return Err("This desktop file or folder is no longer available.".into());
    }
    Ok(())
}

pub struct Application {
    pub desktop_file: PathBuf,
    /// Only for recognizing the existing Snap scope identity, never for launching files.
    pub identity_argv: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub snap_instance: Option<String>,
    pub dbus_activatable: bool,
}

pub fn installed_application(id: &str) -> Result<Application, String> {
    if id.is_empty() || id.contains(['/', '\0']) || !id.ends_with(".desktop") {
        return Err("Invalid installed desktop application ID.".into());
    }
    // MIME handlers may intentionally have NoDisplay=true. The launcher catalog
    // filters those entries, so use GIO's installed desktop-entry lookup here.
    let app = gio_unix::DesktopAppInfo::new(id)
        .filter(|app| !app.is_hidden())
        .ok_or("This application is no longer available.")?;
    let desktop_file = app
        .filename()
        .ok_or("This application has no installed desktop entry.")?;
    let identity_argv = app
        .commandline()
        .map(|command| {
            gio::glib::shell_parse_argv(command)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|argument| {
                    argument
                        .into_string()
                        .map_err(|_| "Invalid desktop command encoding.".into())
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(Application {
        desktop_file,
        identity_argv,
        working_directory: app
            .string("Path")
            .filter(|path| !path.is_empty())
            .map(|path| PathBuf::from(path.as_str())),
        snap_instance: app
            .string("X-SnapInstanceName")
            .map(|value| value.to_string()),
        dbus_activatable: app.boolean("DBusActivatable"),
    })
}

/// The prefix is the compositor's existing scope wrapper. Native paths are
/// appended as OS arguments after it, never formatted into a shell command.
pub fn gio_command(
    desktop_file: &Path,
    paths: &[PathBuf],
    prefix: &[String],
    wayland_display: &str,
    environment: &[(&str, &str)],
) -> Result<Command, String> {
    let mut argv = prefix.to_vec();
    argv.extend(["gio".into(), "launch".into(), "--".into()]);
    let mut command =
        crate::app_catalog::launch_command(&argv, wayland_display, environment, None)?;
    command.arg(desktop_file).args(paths);
    Ok(command)
}

#[cfg(test)]
mod activation_tests;
#[cfg(test)]
mod cold_activation_tests;
#[cfg(test)]
mod tests;
