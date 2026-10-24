use std::{io, process::Command, sync::Arc};

use hearthspace_desktop::app_catalog::reap_child;
use smithay::reexports::calloop::{Interest, Mode as CalloopMode, PostAction, generic::Generic};
use tracing::{debug, error, info, warn};
use wayland_server::ListeningSocket;

use super::{App, launch_environment::*};
use crate::compositor::client::ClientState;
use crate::config::*;

mod apps;
mod display;
mod files;
pub(super) mod pending;
mod snap_scope;

struct ApplicationEnvironment {
    wayland_display: String,
    variables: Vec<(String, String)>,
}

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

    fn finish_application_environment(
        &mut self,
        app_id: &str,
        snap_instance: Option<&str>,
        mut launch_env: Vec<(String, String)>,
    ) -> Result<ApplicationEnvironment, String> {
        let wayland_display = if let Some(instance_name) = snap_instance {
            self.ensure_snap_wayland_listener(instance_name)
                .map_err(|error| format!("failed to prepare Snap Wayland socket: {error}"))?
        } else {
            if let Ok(runtime) = std::env::var(hearthspace_desktop::environment::APP_RUNTIME) {
                launch_env.push(("XDG_RUNTIME_DIR".into(), runtime));
            }
            std::env::var(hearthspace_desktop::environment::APP_DISPLAY)
                .unwrap_or_else(|_| wayland_display_name())
        };
        if app_id == "dev.hearthspace.Settings.desktop" {
            launch_env.push((
                hearthspace_common::SHELL_COMMAND_SOCKET_ENV.to_owned(),
                super::command_socket_path().to_string_lossy().into_owned(),
            ));
        }
        if let Some(display) = self.xwayland_display_name() {
            launch_env.push(("DISPLAY".to_string(), display));
        }
        Ok(ApplicationEnvironment {
            wayland_display,
            variables: launch_env,
        })
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

    pub(in crate::compositor) fn xwayland_display_name(&self) -> Option<String> {
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
