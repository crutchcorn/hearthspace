use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use smithay::{
    backend::input::{ButtonState, KeyState},
    reexports::calloop::{Interest, Mode as CalloopMode, PostAction, generic::Generic},
    utils::Point,
};
use tracing::{debug, error, info, warn};
use wayland_server::ListeningSocket;

use crate::{config::*, geometry::CanvasPoint};
use hearthspace_shell::{
    ShellCommand, SpawnTarget,
    app_catalog::{DesktopApp, spawn_argv_with_env},
};

use super::App;
use super::client::ClientState;

mod command_socket;

pub(in crate::compositor) use command_socket::{
    ControlReply, accept_command_connections, command_socket_path, remove_stale_socket,
};

fn runtime_path(name: &str) -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join(name)
}

fn host_runtime_dir() -> PathBuf {
    env::var_os(hearthspace_common::HOST_RUNTIME_DIR_ENV)
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from))
        .unwrap_or_else(env::temp_dir)
}

/// Snap instance names are embedded into a runtime directory path, so reject any
/// value containing path separators or other characters that could escape the
/// intended `snap.<name>` directory.
fn is_valid_snap_instance_name(instance_name: &str) -> bool {
    instance_name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn snap_wayland_socket_path(
    host_runtime_dir: &Path,
    instance_name: &str,
    wayland_display: &str,
) -> PathBuf {
    host_runtime_dir
        .join(format!("snap.{instance_name}"))
        .join(wayland_display)
}

fn ensure_snap_instance_name(instance_name: &str) -> std::io::Result<()> {
    if !is_valid_snap_instance_name(instance_name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid snap instance name {instance_name:?}"),
        ));
    }

    Ok(())
}

fn launch_environment_for_app(app: &DesktopApp) -> std::io::Result<Vec<(String, String)>> {
    let base = app_state_base_dir().join(sanitized_path_component(&app.id));
    let config = base.join("config");
    let cache = base.join("cache");
    let data = base.join("data");
    let state = base.join("state");

    for dir in [&config, &cache, &data, &state] {
        fs::create_dir_all(dir)?;
    }

    Ok(vec![
        (
            "XDG_CONFIG_HOME".to_string(),
            config.to_string_lossy().into_owned(),
        ),
        (
            "XDG_CACHE_HOME".to_string(),
            cache.to_string_lossy().into_owned(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            data.to_string_lossy().into_owned(),
        ),
        (
            "XDG_STATE_HOME".to_string(),
            state.to_string_lossy().into_owned(),
        ),
    ])
}

fn app_state_base_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(env::temp_dir)
        .join("hearthspace/apps")
}

fn sanitized_path_component(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// Spawn the Xilem-rendered shell (app launcher + compositor controls) as its
/// own Wayland client. The shell tags its surface with [`SHELL_BAR_APP_ID`] so
/// the compositor renders it chrome-less in the bar position.
pub(super) fn spawn_shell(command_socket_path: &PathBuf) {
    let current_exe = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            error!(%error, "failed to locate current executable for shell");
            return;
        }
    };

    if let Err(error) = Command::new(&current_exe)
        .arg(SHELL_FLAG)
        .env(WAYLAND_DISPLAY_ENV, wayland_display_name())
        .env(SHELL_COMMAND_SOCKET_ENV, command_socket_path)
        .spawn()
    {
        error!(path = %current_exe.display(), %error, "failed to spawn shell");
    } else {
        debug!(path = %current_exe.display(), "spawned shell process");
    }
}

#[cfg(feature = "test-apps")]
fn ensure_gtk_client_settings() -> std::io::Result<PathBuf> {
    let config_dir = runtime_path(GTK_CLIENT_CONFIG_DIR_NAME);
    let settings = "[Settings]\ngtk-decoration-layout=:close\n";

    for gtk_version_dir in ["gtk-3.0", "gtk-4.0"] {
        let dir = config_dir.join(gtk_version_dir);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("settings.ini"), settings)?;
    }

    let gsettings_dir = config_dir.join("glib-2.0/settings");
    fs::create_dir_all(&gsettings_dir)?;
    fs::write(
        gsettings_dir.join("keyfile"),
        "[org/gnome/desktop/wm/preferences]\nbutton-layout=':close'\n",
    )?;

    Ok(config_dir)
}

#[cfg(feature = "test-apps")]
fn apply_gtk_client_environment(command: &mut Command) {
    match ensure_gtk_client_settings() {
        Ok(config_dir) => {
            command.env("XDG_CONFIG_HOME", config_dir);
            command.env("GSETTINGS_BACKEND", "keyfile");
        }
        Err(error) => warn!(%error, "failed to prepare GTK client settings"),
    }
}

impl App {
    fn run_control_action(&mut self, action: ShellCommand) -> ControlReply {
        let command = action.wire_name();
        debug!(%command, "running app control action");
        self.advance_viewport_animation();

        match action {
            ShellCommand::Spawn(SpawnTarget::A11yTest) => self.spawn_a11y_test(),
            ShellCommand::Spawn(SpawnTarget::Foot) => self.spawn_foot(),
            ShellCommand::LaunchApp(app_id) => self.launch_app(&app_id, None),
            ShellCommand::LaunchAppOnOutput {
                app_id,
                output_name,
            } => self.launch_app(&app_id, Some(&output_name)),
            ShellCommand::PanLeft => self.pan_viewport_by(-self.horizontal_pan_step(), 0),
            ShellCommand::PanRight => self.pan_viewport_by(self.horizontal_pan_step(), 0),
            ShellCommand::PanUp => self.pan_viewport_by(0, -self.vertical_pan_step()),
            ShellCommand::PanDown => self.pan_viewport_by(0, self.vertical_pan_step()),
            ShellCommand::ZoomIn => self.animate_zoom_around_viewport_center(ZOOM_STEP),
            ShellCommand::ZoomOut => self.animate_zoom_around_viewport_center(1.0 / ZOOM_STEP),
            ShellCommand::ReloadSettings => self.reload_settings(),
            ShellCommand::LogAccessibilityTree => {
                crate::accessibility::log_accessibility_tree(self.accessibility_window_snapshot())
            }
            ShellCommand::KeyDown(keycode) => self.synthesize_key(keycode, KeyState::Pressed),
            ShellCommand::KeyUp(keycode) => self.synthesize_key(keycode, KeyState::Released),
            ShellCommand::PointerMotionAbs { x, y } => {
                self.synthesize_pointer_motion_abs(Point::from((x, y)))
            }
            ShellCommand::PointerMotionRel { dx, dy } => {
                self.synthesize_pointer_motion_rel(Point::from((dx, dy)))
            }
            ShellCommand::PointerButtonDown(button) => {
                self.synthesize_pointer_button(button, ButtonState::Pressed)
            }
            ShellCommand::PointerButtonUp(button) => {
                self.synthesize_pointer_button(button, ButtonState::Released)
            }
            ShellCommand::Axis {
                horizontal,
                vertical,
            } => self.synthesize_axis(horizontal, vertical),
            ShellCommand::Screenshot => unreachable!("screenshot is handled by CalloopData"),
            ShellCommand::Quit => unreachable!("quit is handled by CalloopData"),
        }
        self.request_redraw();
        ControlReply::Ok
    }

    fn reload_settings(&mut self) {
        match hearthspace_ipc::client_get_settings() {
            Ok(settings) => {
                let refresh = self.apply_settings(settings);
                debug!(
                    output_layout_changed = refresh.output_layout_changed,
                    background_changed = refresh.background_changed,
                    "reloaded compositor settings"
                );
            }
            Err(error) => warn!(%error, "failed to reload compositor settings"),
        }
    }

    fn prepare_spawn_position(&mut self) {
        self.prepare_spawn_position_for_output(None);
    }

    fn prepare_spawn_position_for_output(&mut self, output_name: Option<&str>) {
        let resolved_output_name = match output_name {
            Some(name) if self.output_logical_rect_for_name(Some(name)).is_some() => {
                Some(name.to_string())
            }
            Some(name) => {
                warn!(
                    output_name = name,
                    "launch output no longer exists; using primary output"
                );
                None
            }
            None => None,
        };
        let output_rect = self
            .output_logical_rect_for_name(resolved_output_name.as_deref())
            .unwrap_or_else(|| self.primary_output_logical_rect());
        let output_center = Point::from((
            f64::from(output_rect.loc.x) + f64::from(output_rect.size.w) / 2.0,
            f64::from(output_rect.loc.y) + f64::from(output_rect.size.h) / 2.0,
        ));
        let canvas_center = self.screen_to_canvas(output_center);
        self.next_spawn_position = CanvasPoint {
            x: canvas_center.x.round() as i32 - MIN_WINDOW_WIDTH / 2 + self.spawn_offset,
            y: canvas_center.y.round() as i32 - 180 + self.spawn_offset,
        };
        self.next_spawn_output_name = resolved_output_name;
        self.spawn_offset = (self.spawn_offset + SPAWN_OFFSET_STEP) % SPAWN_OFFSET_WRAP;
    }

    fn spawn_a11y_test(&mut self) {
        #[cfg(feature = "test-apps")]
        {
            self.prepare_spawn_position();

            let current_exe = match env::current_exe() {
                Ok(path) => path,
                Err(error) => {
                    error!(%error, "failed to locate current executable for GTK test app");
                    return;
                }
            };

            let mut command = Command::new(current_exe);
            command
                .arg(GTK_TEST_APP_FLAG)
                .env(WAYLAND_DISPLAY_ENV, wayland_display_name())
                .env("GDK_BACKEND", "wayland");
            apply_gtk_client_environment(&mut command);

            match command.spawn() {
                Ok(child) => info!(pid = child.id(), "spawned GTK test app"),
                Err(error) => error!(%error, "failed to spawn GTK test app"),
            }
        }

        #[cfg(not(feature = "test-apps"))]
        warn!("GTK test app support is not enabled; rebuild with `--features test-apps`");
    }

    fn spawn_foot(&mut self) {
        self.prepare_spawn_position();

        match Command::new("foot")
            .env(WAYLAND_DISPLAY_ENV, wayland_display_name())
            .spawn()
        {
            Ok(child) => info!(pid = child.id(), "spawned foot terminal"),
            Err(error) => error!(%error, "failed to spawn foot"),
        }
    }

    fn launch_app(&mut self, app_id: &str, output_name: Option<&str>) {
        let Some(app) = self.app_catalog.app_by_id(app_id).cloned() else {
            warn!(app_id, "no launchable desktop app found");
            return;
        };
        info!(app_id, name = %app.name, terminal = app.terminal, ?output_name, "launching desktop app");

        let app_command = match app.launch_argv() {
            Ok(command) => command,
            Err(error) => {
                error!(app_id, %error, "failed to build desktop app command");
                return;
            }
        };
        let command = if app.terminal {
            match self.app_catalog.terminal_command_for(app_command) {
                Ok(command) => command,
                Err(error) => {
                    error!(app_id, %error, "failed to find terminal for desktop app");
                    return;
                }
            }
        } else {
            app_command
        };

        let wayland_display = if let Some(instance_name) = &app.snap_instance_name {
            match self.ensure_snap_wayland_listener(instance_name) {
                Ok(display_name) => display_name,
                Err(error) => {
                    error!(app_id, %error, "failed to prepare Snap Wayland socket");
                    return;
                }
            }
        } else {
            wayland_display_name()
        };

        let launch_env = match launch_environment_for_app(&app) {
            Ok(env) => env,
            Err(error) => {
                error!(app_id, %error, "failed to prepare app environment");
                return;
            }
        };
        let launch_env_refs = launch_env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();

        self.prepare_spawn_position_for_output(output_name);
        if let Err(error) = spawn_argv_with_env(&command, &wayland_display, &launch_env_refs) {
            error!(app_id, %error, "failed to launch desktop app");
        } else {
            info!(app_id, wayland_display, argv = ?command, "launched desktop app");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitized_path_component_replaces_unsafe_characters() {
        assert_eq!(
            sanitized_path_component("org.gnome.Calculator"),
            "org.gnome.Calculator"
        );
        // Path separators become underscores; dots are preserved.
        assert_eq!(sanitized_path_component("a/../b"), "a_.._b");
        assert_eq!(sanitized_path_component("weird name!"), "weird_name_");
    }

    #[test]
    fn snap_instance_names_reject_path_separators() {
        assert!(is_valid_snap_instance_name("firefox"));
        assert!(is_valid_snap_instance_name("foo_bar-1.2"));
        assert!(!is_valid_snap_instance_name("../escape"));
        assert!(!is_valid_snap_instance_name("with/slash"));
        assert!(!is_valid_snap_instance_name("space here"));
    }

    #[test]
    fn snap_wayland_socket_path_uses_host_snap_runtime() {
        let socket_path =
            snap_wayland_socket_path(Path::new("/run/user/1000"), "firefox", "wayland-99");

        assert_eq!(
            socket_path,
            PathBuf::from("/run/user/1000/snap.firefox/wayland-99")
        );
    }
}
