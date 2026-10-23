use super::{App, snap_scope};
use crate::compositor::shell_integration::{
    ControlReply, launch_environment::launch_environment_for_app,
};
use hearthspace_shell::desktop_items::file_launch::{self, Request};
use std::{path::PathBuf, process::Command, time::Duration};

mod pending;

struct Prepared {
    request: Request,
    paths: Vec<PathBuf>,
    app: file_launch::Application,
    snap_instance: Option<String>,
    prefix: Vec<String>,
    environment: Vec<(String, String)>,
    activation_helper: Option<PathBuf>,
}

/// All probes of user-selected files, desktop directories and executable paths
/// happen on the bounded worker, including profile and scope-record writes.
fn prepare(request: Request) -> Result<Prepared, String> {
    let paths = request.paths()?;
    let mut app = file_launch::installed_application(&request.app_id)?;
    let snap_instance = snap_scope::resolve(
        app.snap_instance.as_deref(),
        &mut app.identity_argv,
        app.working_directory.as_deref(),
    )?;
    let prefix = snap_instance
        .as_deref()
        .map(|instance| snap_scope::prefix(instance, &app.identity_argv))
        .transpose()?
        .unwrap_or_default();
    let environment = launch_environment_for_app(&request.app_id)
        .map_err(|error| format!("failed to prepare app environment: {error}"))?;
    let activation_helper = app
        .dbus_activatable
        .then(std::env::current_exe)
        .transpose()
        .map_err(|error| format!("Could not locate file launcher: {error}"))?;
    Ok(Prepared {
        request,
        paths,
        app,
        snap_instance,
        prefix,
        environment,
        activation_helper,
    })
}

impl App {
    pub(in crate::compositor::shell_integration) fn launch_files(
        &mut self,
        text: &str,
    ) -> ControlReply {
        let request = match serde_json::from_str::<Request>(text) {
            Ok(request) => request,
            Err(error) => {
                return ControlReply::Err(format!("Invalid file launch request: {error}"));
            }
        };
        pending::start(
            &self.loop_handle,
            Duration::from_secs(8),
            move || prepare(request),
            |prepared, _, data| data.state.configure_file_launch(prepared),
        )
    }

    fn configure_file_launch(&mut self, prepared: Prepared) -> Result<Command, String> {
        if self.session_locked() {
            return Err("session is locked".into());
        }
        if let Some(name) = prepared.request.output_name.as_deref()
            && self.output_logical_rect_for_name(Some(name)).is_none()
        {
            return Err(format!("unknown launch output: {name}"));
        }
        // Only compositor-owned local socket registration and current output /
        // XWayland state require the event loop. The worker prepared all paths.
        let environment = self.finish_application_environment(
            &prepared.request.app_id,
            prepared.snap_instance.as_deref(),
            prepared.environment,
        )?;
        let envs = environment
            .variables
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let command = if let Some(helper) = &prepared.activation_helper {
            if prepared.snap_instance.is_some() {
                file_launch::scoped_gio_command(
                    &prepared.app.desktop_file,
                    &prepared.paths,
                    &prepared.prefix,
                    &environment.wayland_display,
                    &envs,
                    helper,
                )?
            } else {
                file_launch::native_gio_command(
                    &prepared.app.desktop_file,
                    &prepared.paths,
                    &environment.wayland_display,
                    &envs,
                    helper,
                )?
            }
        } else {
            file_launch::gio_command(
                &prepared.app.desktop_file,
                &prepared.paths,
                &prepared.prefix,
                &environment.wayland_display,
                &envs,
            )?
        };
        self.prepare_spawn_output(prepared.request.output_name.as_deref());
        Ok(command)
    }
}
