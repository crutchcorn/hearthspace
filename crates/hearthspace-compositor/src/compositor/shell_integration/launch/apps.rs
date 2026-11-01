//! Installed app preparation shares the file launcher's deadline and admission.
use super::pending::Execution;
use super::{App, pending, snap_scope};
use crate::compositor::{
    CalloopData,
    shell_integration::{ControlReply, launch_environment::launch_environment_for_app},
};
use hearthspace_desktop::{
    app_catalog::{AppCatalog, launch_command},
    file_launch,
};
use std::time::Duration;

const SETTINGS: &str = "dev.hearthspace.Settings.desktop";

struct Prepared {
    app_id: String,
    catalog: AppCatalog,
    snap_instance: Option<String>,
    execution: Execution,
    environment: Vec<(String, String)>,
}

fn prepare(app_id: String, arguments: Vec<String>) -> Result<Prepared, String> {
    prepare_with_gpu(
        app_id,
        arguments,
        hearthspace_desktop::gpu::launch_environment,
    )
}

fn prepare_with_gpu(
    app_id: String,
    arguments: Vec<String>,
    resolve_gpu: impl FnOnce(bool) -> Vec<(String, String)>,
) -> Result<Prepared, String> {
    let catalog = AppCatalog::load();
    let app = catalog
        .app_by_id(&app_id)
        .ok_or_else(|| format!("unknown desktop application: {app_id}"))?;
    let mut argv = if app.exec.is_empty() {
        Vec::new()
    } else {
        app.launch_argv()?
    };
    argv.extend(arguments.iter().cloned());
    let snap_instance = snap_scope::resolve(
        app.snap_instance_name.as_deref(),
        &mut argv,
        app.working_directory.as_deref(),
    )?;
    let mut environment = launch_environment_for_app(&app_id)
        .map_err(|error| format!("failed to prepare app environment: {error}"))?;
    // A D-Bus-only entry has no child command whose environment we can own.
    // Keep its ordinary activation; the desktop preference is best effort.
    let gpu_environment = if argv.is_empty() {
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
    let selected_bus =
        std::env::var_os("HEARTHSPACE_SESSION_BUS_ADDRESS").is_some_and(|bus| !bus.is_empty());
    // These explicit argv routes preserve terminal preference, app arguments,
    // sibling development binaries and isolated profiles. Ordinary native apps
    // use GIO's Activate/Exec decision and wait for actual activation acceptance.
    let direct = app.terminal
        || !arguments.is_empty()
        || app_id == SETTINGS
        || app.exec.split_whitespace().next() == Some("hearthspace-ui-storybook")
        || !selected_bus
        || environment
            .iter()
            .any(|(key, _)| key == "GSETTINGS_BACKEND");
    let execution = if direct {
        if argv.is_empty() {
            return Err("This application requires desktop bus activation.".into());
        }
        if let Some(instance) = &snap_instance {
            argv = snap_scope::wrap(instance, argv)?;
        }
        if app.terminal {
            argv = catalog.terminal_command_for(argv)?;
        }
        Execution::detached(launch_command(
            &argv,
            "",
            &envs,
            app.working_directory.as_deref(),
        )?)
    } else {
        let helper = std::env::current_exe().map_err(|error| error.to_string())?;
        let command = if snap_instance.is_some() || needs_gpu_environment {
            let prefix = snap_instance
                .as_deref()
                .map(|instance| snap_scope::prefix(instance, &argv))
                .transpose()?
                .unwrap_or_default();
            file_launch::scoped_gio_command(&app.path, &[], &prefix, "", &envs, &helper)?
        } else {
            file_launch::native_gio_command(&app.path, &[], "", &envs, &helper)?
        };
        Execution::helper(command)
    };
    Ok(Prepared {
        app_id,
        catalog,
        snap_instance,
        execution,
        environment,
    })
}

impl App {
    pub(in crate::compositor) fn launch_app_reply(
        &mut self,
        app_id: &str,
        output: Option<&str>,
    ) -> ControlReply {
        let existing = (app_id == SETTINGS)
            .then(|| self.existing_settings_window())
            .flatten();
        self.launch_app_with_arguments(app_id, output, &[], existing, None)
    }

    fn existing_settings_window(&self) -> Option<u64> {
        self.windows.iter().enumerate().find_map(|(index, window)| {
            (self.is_navigable_window(index)
                && crate::compositor::windows::window_app_id(&window.surface).as_deref()
                    == Some("dev.hearthspace.Settings"))
            .then_some(window.id)
        })
    }

    pub(in crate::compositor) fn launch_drawing_reply(
        &mut self,
        app_id: &str,
        output: Option<&str>,
        generation: u64,
    ) -> ControlReply {
        self.launch_app_with_arguments(app_id, output, &[], None, Some(generation))
    }

    pub(in crate::compositor::shell_integration) fn open_search_settings_reply(
        &mut self,
        output: Option<&str>,
    ) -> ControlReply {
        self.settings_page_reply(output, "gestures")
    }

    pub(in crate::compositor) fn open_background_settings_reply(
        &mut self,
        output: Option<&str>,
    ) -> ControlReply {
        self.settings_page_reply(output, "background")
    }

    fn settings_page_reply(&mut self, output: Option<&str>, page: &str) -> ControlReply {
        self.launch_app_with_arguments(
            SETTINGS,
            output,
            &["--page", page],
            self.existing_settings_window(),
            None,
        )
    }

    pub(in crate::compositor) fn open_layout_settings(
        &mut self,
        output: Option<&str>,
    ) -> Result<(), String> {
        // Internal compositor actions cannot await a reply. Failures are still
        // logged by the shared worker; external launch requests retain the reply.
        match self.settings_page_reply(output, "system") {
            ControlReply::Err(error) => Err(error),
            _ => Ok(()),
        }
    }

    fn launch_app_with_arguments(
        &mut self,
        app_id: &str,
        output: Option<&str>,
        arguments: &[&str],
        existing: Option<u64>,
        drawing: Option<u64>,
    ) -> ControlReply {
        let app_id = app_id.to_owned();
        let arguments = arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect();
        let output = output.map(str::to_owned);
        pending::start_checked(
            &self.loop_handle,
            Duration::from_secs(8),
            move || prepare(app_id, arguments),
            |mut prepared, _, data| {
                let environment = data.state.finish_application_environment(
                    &prepared.app_id,
                    prepared.snap_instance.as_deref(),
                    prepared.environment,
                )?;
                let command = &mut prepared.execution.command;
                command
                    .env("WAYLAND_DISPLAY", environment.wayland_display)
                    .envs(environment.variables);
                prepared.execution.display =
                    super::display::snapshot(data.state.xwayland_display_name());
                data.state.app_catalog = prepared.catalog;
                Ok(prepared.execution)
            },
            move |display, data| {
                check_display(display, data)?;
                admit(data, output.as_deref())?;
                if let Some(generation) = drawing {
                    data.state.validate_drawing_launch(generation)?;
                }
                if existing.is_none() {
                    data.state.prepare_spawn_output(output.as_deref());
                }
                Ok(())
            },
            move |result, data| {
                result?;
                if let Some(id) = existing
                    && !data.state.session_locked()
                {
                    data.state.focus_window(id)?;
                }
                Ok(())
            },
        )
    }
}

pub(super) fn admit(data: &CalloopData, output: Option<&str>) -> Result<(), String> {
    if !data.running || !data.capture_active() || data.state.session_locked() {
        return Err("session is inactive or locked".into());
    }
    if let Some(name) = output
        && data
            .state
            .output_logical_rect_for_name(Some(name))
            .is_none()
    {
        return Err(format!("unknown launch output: {name}"));
    }
    Ok(())
}

pub(super) fn check_display(
    update: Option<hearthspace_desktop::activation::DisplayUpdate>,
    data: &CalloopData,
) -> Result<(), String> {
    if update
        .is_some_and(|update| !super::display::current(&update, data.state.xwayland_display_name()))
    {
        return Err("Application display changed while preparing launch. Try again.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
