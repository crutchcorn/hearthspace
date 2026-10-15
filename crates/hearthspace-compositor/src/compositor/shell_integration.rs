use std::{env, path::PathBuf, process::Command};

use hearthspace_shell::{ShellCommand, SpawnTarget};
use smithay::{
    backend::input::{ButtonState, KeyState},
    utils::Point,
};
use tracing::{debug, error, warn};

use super::App;
use crate::config::*;

mod command_socket;
mod launch;
mod launch_environment;

pub(in crate::compositor) use command_socket::{
    ControlReply, accept_command_connections, command_socket_path, remove_stale_socket,
};

fn runtime_path(name: &str) -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join(name)
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

    match Command::new(&current_exe)
        .arg(SHELL_FLAG)
        .env(WAYLAND_DISPLAY_ENV, wayland_display_name())
        .env(SHELL_COMMAND_SOCKET_ENV, command_socket_path)
        .spawn()
    {
        Ok(child) => {
            hearthspace_shell::app_catalog::reap_child(child);
            debug!(path = %current_exe.display(), "spawned shell process");
        }
        Err(error) => error!(path = %current_exe.display(), %error, "failed to spawn shell"),
    }
}

impl App {
    pub(super) fn run_control_action(&mut self, action: ShellCommand) -> ControlReply {
        if self.session_locked() {
            return ControlReply::Err("session is locked".into());
        }
        let command = action.wire_name();
        debug!(%command, "running app control action");
        self.advance_viewport_animation();

        if (self.overview_active() || self.window_switcher_active())
            && matches!(
                action,
                ShellCommand::PanLeft
                    | ShellCommand::PanRight
                    | ShellCommand::PanUp
                    | ShellCommand::PanDown
                    | ShellCommand::PanTo { .. }
                    | ShellCommand::ZoomIn
                    | ShellCommand::ZoomOut
            )
        {
            return ControlReply::Err(
                "dismiss the window overlay before changing the canvas view".into(),
            );
        }

        match action {
            ShellCommand::OverviewOn => return action_reply(self.open_overview()),
            ShellCommand::OverviewOff => {
                self.close_overview();
                return ControlReply::Ok;
            }
            ShellCommand::OverviewToggle => return action_reply(self.toggle_overview()),
            ShellCommand::OverviewSelect(id) => {
                return action_reply(self.select_overview_window(id));
            }
            ShellCommand::LockSession => return action_reply(self.launch_session_locker()),
            ShellCommand::ListWindows => return ControlReply::Payload(self.window_list_json()),
            ShellCommand::FocusWindow(id) => return action_reply(self.focus_window(id)),
            ShellCommand::CloseWindow(id) => return action_reply(self.close_window(id)),
            ShellCommand::MoveWindow { id, x, y } => {
                return action_reply(self.move_window(id, x, y));
            }
            ShellCommand::ResizeWindow { id, width, height } => {
                return action_reply(self.resize_window(id, width, height));
            }
            ShellCommand::FocusNextWindow => return action_reply(self.cycle_window_focus(false)),
            ShellCommand::FocusPreviousWindow => {
                return action_reply(self.cycle_window_focus(true));
            }
            ShellCommand::FocusLauncher => return action_reply(self.focus_launcher()),
            ShellCommand::ResetView => return action_reply(self.reset_view()),
            ShellCommand::FullscreenWindow { id, enabled } => {
                return action_reply(self.user_window_presentation(id, Some(enabled), None));
            }
            ShellCommand::MaximizeWindow { id, enabled } => {
                return action_reply(self.user_window_presentation(id, None, Some(enabled)));
            }
            ShellCommand::RestoreWindow(id) => {
                return action_reply(self.user_window_presentation(id, Some(false), Some(false)));
            }
            ShellCommand::Spawn(SpawnTarget::A11yTest) => {
                return self
                    .spawn_a11y_test()
                    .map_or_else(ControlReply::Err, |_| ControlReply::Ok);
            }
            ShellCommand::Spawn(SpawnTarget::Foot) => {
                return self
                    .spawn_foot()
                    .map_or_else(ControlReply::Err, |_| ControlReply::Ok);
            }
            ShellCommand::LaunchApp(app_id) => {
                return self
                    .launch_app(&app_id, None)
                    .map_or_else(ControlReply::Err, |_| ControlReply::Ok);
            }
            ShellCommand::ListApps => return ControlReply::Payload(self.list_apps_json()),
            ShellCommand::LaunchAppOnOutput {
                app_id,
                output_name,
            } => {
                return self
                    .launch_app(&app_id, Some(&output_name))
                    .map_or_else(ControlReply::Err, |_| ControlReply::Ok);
            }
            ShellCommand::PanLeft => self.pan_viewport_by(-self.horizontal_pan_step(), 0),
            ShellCommand::PanRight => self.pan_viewport_by(self.horizontal_pan_step(), 0),
            ShellCommand::PanUp => self.pan_viewport_by(0, -self.vertical_pan_step()),
            ShellCommand::PanDown => self.pan_viewport_by(0, self.vertical_pan_step()),
            ShellCommand::PanTo { x, y } => return action_reply(self.pan_viewport_to(x, y)),
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
}

fn action_reply(result: Result<(), String>) -> ControlReply {
    result.map_or_else(ControlReply::Err, |_| ControlReply::Ok)
}
