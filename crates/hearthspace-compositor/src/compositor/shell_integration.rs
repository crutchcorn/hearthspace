use std::{
    env, fs,
    io::{self, ErrorKind},
    os::unix::fs::symlink,
    path::PathBuf,
    process::Command,
};

use smithay::{
    backend::input::{ButtonState, KeyState},
    utils::Point,
};
use tracing::{debug, error, info, warn};

use crate::{config::*, geometry::CanvasPoint};
use hearthspace_shell::{
    ShellCommand, SpawnTarget,
    app_catalog::{DesktopApp, spawn_argv_with_env},
};

use super::App;

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

/// Snap instance names are embedded into a runtime directory path, so reject any
/// value containing path separators or other characters that could escape the
/// intended `snap.<name>` directory.
fn is_valid_snap_instance_name(instance_name: &str) -> bool {
    instance_name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn ensure_snap_wayland_socket(instance_name: &str) -> std::io::Result<String> {
    if !is_valid_snap_instance_name(instance_name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("invalid snap instance name {instance_name:?}"),
        ));
    }

    let snap_runtime_dir = runtime_path(&format!("snap.{instance_name}"));
    fs::create_dir_all(&snap_runtime_dir)?;
    let wayland_display = wayland_display_name();
    let snap_socket_path = snap_runtime_dir.join(&wayland_display);

    match fs::remove_file(&snap_socket_path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    symlink(
        PathBuf::from("..").join(&wayland_display),
        &snap_socket_path,
    )?;
    Ok(wayland_display)
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
            ShellCommand::LaunchApp(app_id) => self.launch_app(&app_id),
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
        let output_size = self.output_size();
        self.next_spawn_position = CanvasPoint {
            x: self.viewport_offset.x
                + (f64::from(output_size.w) / 2.0 / self.viewport_scale).round() as i32
                - MIN_WINDOW_WIDTH / 2
                + self.spawn_offset,
            y: self.viewport_offset.y
                + (f64::from(output_size.h) / 2.0 / self.viewport_scale).round() as i32
                - 180
                + self.spawn_offset,
        };
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

    fn launch_app(&mut self, app_id: &str) {
        let Some(app) = self.app_catalog.app_by_id(app_id) else {
            warn!(app_id, "no launchable desktop app found");
            return;
        };
        info!(app_id, name = %app.name, terminal = app.terminal, "launching desktop app");

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
            match ensure_snap_wayland_socket(instance_name) {
                Ok(display_name) => display_name,
                Err(error) => {
                    error!(app_id, %error, "failed to prepare Snap Wayland socket");
                    return;
                }
            }
        } else {
            wayland_display_name()
        };

        let launch_env = match launch_environment_for_app(app) {
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

        self.prepare_spawn_position();
        if let Err(error) = spawn_argv_with_env(&command, &wayland_display, &launch_env_refs) {
            error!(app_id, %error, "failed to launch desktop app");
        } else {
            info!(app_id, wayland_display, argv = ?command, "launched desktop app");
        }
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
}
