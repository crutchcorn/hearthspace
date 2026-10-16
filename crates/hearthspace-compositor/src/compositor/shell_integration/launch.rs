use std::{fs, io, process::Command, sync::Arc};

use hearthspace_shell::app_catalog::{AppCatalog, reap_child, spawn_argv_with_options};
use smithay::reexports::calloop::{Interest, Mode as CalloopMode, PostAction, generic::Generic};
use tracing::{debug, error, info, warn};
use wayland_server::ListeningSocket;

use super::{App, launch_environment::*};
use crate::compositor::client::ClientState;
use crate::config::*;

mod snap_scope;

impl App {
    fn prepare_spawn_output(&mut self, output_name: Option<&str>) {
        let resolved_output_name = match output_name {
            Some(name) if self.output_logical_rect_for_name(Some(name)).is_some() => {
                Some(name.to_string())
            }
            Some(name) => {
                warn!(
                    output_name = name,
                    "launch output no longer exists; using current output"
                );
                None
            }
            None => None,
        };
        // Actual client geometry is only available when its first buffer maps.
        // Retain the output hint, then center that geometry in the current view.
        self.next_spawn_output_name = resolved_output_name;
    }

    pub(super) fn spawn_a11y_test(&mut self) -> Result<(), String> {
        #[cfg(feature = "test-apps")]
        {
            self.prepare_spawn_output(None);

            let current_exe = std::env::current_exe()
                .map_err(|error| format!("failed to locate GTK test app: {error}"))?;

            let mut command = Command::new(current_exe);
            command
                .arg(GTK_TEST_APP_FLAG)
                .env(WAYLAND_DISPLAY_ENV, wayland_display_name())
                .env("GDK_BACKEND", "wayland");
            apply_gtk_client_environment(&mut command);

            let child = command
                .spawn()
                .map_err(|error| format!("failed to spawn GTK test app: {error}"))?;
            let pid = reap_child(child);
            info!(pid, "spawned GTK test app");
            Ok(())
        }

        #[cfg(not(feature = "test-apps"))]
        Err("GTK test app support is not enabled; rebuild with `--features test-apps`".into())
    }

    pub(super) fn spawn_foot(&mut self) -> Result<(), String> {
        self.prepare_spawn_output(None);

        let child = Command::new("foot")
            .env(WAYLAND_DISPLAY_ENV, wayland_display_name())
            .spawn()
            .map_err(|error| format!("failed to spawn foot: {error}"))?;
        let pid = reap_child(child);
        info!(pid, "spawned foot terminal");
        Ok(())
    }

    pub(in crate::compositor) fn launch_app(
        &mut self,
        app_id: &str,
        output_name: Option<&str>,
    ) -> Result<(), String> {
        self.launch_app_with_arguments(app_id, output_name, &[])
    }

    pub(super) fn open_search_settings(&mut self, output_name: Option<&str>) -> Result<(), String> {
        self.launch_app_with_arguments(
            "dev.hearthspace.Settings.desktop",
            output_name,
            &["--page", "search"],
        )
    }

    fn launch_app_with_arguments(
        &mut self,
        app_id: &str,
        output_name: Option<&str>,
        arguments: &[&str],
    ) -> Result<(), String> {
        // Resolve current installed entries so new favorites work and removed apps cannot launch stale commands.
        self.app_catalog = AppCatalog::load();
        let app = self
            .app_catalog
            .app_by_id(app_id)
            .cloned()
            .ok_or_else(|| format!("unknown desktop application: {app_id}"))?;
        if let Some(name) = output_name
            && self.output_logical_rect_for_name(Some(name)).is_none()
        {
            return Err(format!("unknown launch output: {name}"));
        }
        let mut app_command = app.launch_argv()?;
        app_command.extend(arguments.iter().map(|argument| (*argument).to_string()));
        let app_command = if let Some(instance) = &app.snap_instance_name {
            snap_scope::wrap(instance, app_command)?
        } else {
            app_command
        };
        let command = if app.terminal {
            self.app_catalog.terminal_command_for(app_command)?
        } else {
            app_command
        };
        let wayland_display = if let Some(instance_name) = &app.snap_instance_name {
            self.ensure_snap_wayland_listener(instance_name)
                .map_err(|error| format!("failed to prepare Snap Wayland socket: {error}"))?
        } else {
            wayland_display_name()
        };
        let mut launch_env = launch_environment_for_app(&app)
            .map_err(|error| format!("failed to prepare app environment: {error}"))?;
        if let Some(display) = self.xwayland_display_name() {
            launch_env.push(("DISPLAY".to_string(), display));
        }
        let launch_env_refs = launch_env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        self.prepare_spawn_output(output_name);
        spawn_argv_with_options(
            &command,
            &wayland_display,
            &launch_env_refs,
            app.working_directory.as_deref(),
        )?;
        info!(app_id, wayland_display, argv = ?command, "launched desktop app");
        Ok(())
    }

    pub(super) fn list_apps_json(&self) -> Vec<u8> {
        let apps: Vec<_> = self
            .app_catalog
            .apps()
            .iter()
            .map(|app| {
                serde_json::json!({
                    "id": app.id, "name": app.name, "generic_name": app.generic_name,
                    "comment": app.comment, "terminal": app.terminal, "categories": app.categories,
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({"version": 1, "apps": apps}))
            .expect("serializable app catalog")
    }

    fn xwayland_display_name(&self) -> Option<String> {
        #[cfg(feature = "xwayland")]
        {
            self.xdisplay
                .map(|display_number| format!(":{display_number}"))
        }
        #[cfg(not(feature = "xwayland"))]
        {
            None
        }
    }

    fn ensure_snap_wayland_listener(&mut self, instance_name: &str) -> io::Result<String> {
        ensure_snap_instance_name(instance_name)?;

        let wayland_display = wayland_display_name();
        let socket_path =
            snap_wayland_socket_path(&host_runtime_dir(), instance_name, &wayland_display);
        if self.snap_wayland_socket_paths.contains(&socket_path) {
            return Ok(wayland_display);
        }
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let socket = ListeningSocket::bind_absolute(socket_path.clone()).map_err(|error| {
            io::Error::other(format!(
                "failed to bind Snap Wayland socket {}: {error}",
                socket_path.display()
            ))
        })?;
        let source = Generic::new(socket, Interest::READ, CalloopMode::Level);
        let log_path = socket_path.clone();
        self.loop_handle
            .insert_source(source, move |_, socket, data| {
                while let Some(stream) = socket.accept()? {
                    match data
                        .display
                        .handle()
                        .insert_client(stream, Arc::new(ClientState::default()))
                    {
                        Ok(client_id) => {
                            debug!(?client_id, socket = %log_path.display(), "accepted Snap Wayland client")
                        }
                        Err(error) => error!(
                            %error,
                            socket = %log_path.display(),
                            "failed to insert Snap Wayland client"
                        ),
                    }
                    data.state.request_redraw();
                }
                Ok(PostAction::Continue)
            })
            .map_err(|error| {
                io::Error::other(format!(
                    "failed to register Snap Wayland socket {}: {error}",
                    socket_path.display()
                ))
            })?;

        info!(
            socket = %socket_path.display(),
            instance_name,
            wayland_display,
            "listening for Snap Wayland clients"
        );
        self.snap_wayland_socket_paths.insert(socket_path);
        Ok(wayland_display)
    }
}
