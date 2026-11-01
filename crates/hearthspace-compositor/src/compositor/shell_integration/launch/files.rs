use super::pending::Execution;
use super::{App, apps, pending, snap_scope};
use crate::compositor::shell_integration::{
    ControlReply, launch_environment::launch_environment_for_app,
};
use hearthspace_desktop::file_launch::{self, Request};
use std::{process::Command, time::Duration};

struct Prepared {
    request: Request,
    snap_instance: Option<String>,
    environment: Vec<(String, String)>,
    command: Command,
}

/// All probes of user-selected files, desktop directories and executable paths
/// happen on the bounded worker, including profile and scope-record writes.
fn prepare(request: Request) -> Result<Prepared, String> {
    prepare_with_gpu(request, hearthspace_desktop::gpu::launch_environment)
}

fn prepare_with_gpu(
    request: Request,
    resolve_gpu: impl FnOnce(bool) -> Vec<(String, String)>,
) -> Result<Prepared, String> {
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
    let mut environment = launch_environment_for_app(&request.app_id)
        .map_err(|error| format!("failed to prepare app environment: {error}"))?;
    let gpu_environment = if app.identity_argv.is_empty() {
        Vec::new()
    } else {
        resolve_gpu(app.prefers_non_default_gpu)
    };
    let needs_gpu_environment = !gpu_environment.is_empty();
    environment.extend(gpu_environment);
    let envs = environment
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect::<Vec<_>>();
    let command = if app.dbus_activatable {
        let helper = std::env::current_exe().map_err(|error| error.to_string())?;
        if snap_instance.is_some()
            || needs_gpu_environment
            || environment
                .iter()
                .any(|(key, _)| key == "GSETTINGS_BACKEND")
        {
            file_launch::scoped_gio_command(&app.desktop_file, &paths, &prefix, "", &envs, &helper)?
        } else {
            file_launch::native_gio_command(&app.desktop_file, &paths, "", &envs, &helper)?
        }
    } else {
        file_launch::gio_command(&app.desktop_file, &paths, &prefix, "", &envs)?
    };
    Ok(Prepared {
        request,
        snap_instance,
        environment,
        command,
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
        let output = request.output_name.clone();
        pending::start_checked(
            &self.loop_handle,
            Duration::from_secs(8),
            move || prepare(request),
            |prepared, _, data| data.state.configure_file_launch(prepared),
            move |display, data| {
                apps::check_display(display, data)?;
                apps::admit(data, output.as_deref())?;
                data.state.prepare_spawn_output(output.as_deref());
                Ok(())
            },
            |result, _| result,
        )
    }

    fn configure_file_launch(&mut self, mut prepared: Prepared) -> Result<Execution, String> {
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
        prepared
            .command
            .env("WAYLAND_DISPLAY", environment.wayland_display)
            .envs(environment.variables);
        let mut execution = Execution::helper(prepared.command);
        execution.display = super::display::snapshot(self.xwayland_display_name());
        Ok(execution)
    }
}

#[cfg(test)]
pub(super) mod tests;
