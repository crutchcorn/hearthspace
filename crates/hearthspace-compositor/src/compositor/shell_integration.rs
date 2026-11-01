use std::{env, path::PathBuf, process::Command};

use hearthspace_ipc::command::{ShellCommand, SpawnTarget};
use smithay::{
    backend::input::{ButtonState, KeyState},
    utils::Point,
};
use tracing::{debug, error};

use super::App;
use crate::config::*;

mod command_socket;
pub(in crate::compositor) mod device_control;
mod favorite_drawing;
#[cfg(test)]
mod keyboard_tests;
mod launch;
mod launch_environment;
pub(in crate::compositor) mod search;
mod touch_control;

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
            hearthspace_desktop::app_catalog::reap_child(child);
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
        // Qualified zoom validates its output before sampling the viewport.
        if !matches!(action, ShellCommand::ZoomOnOutput { .. }) {
            self.advance_viewport_animation();
        }

        if (self.canvas_search_active()
            || self.window_switcher_active()
            || self.modal_confirmation_active()
            || self.group_interaction_active())
            && matches!(
                action,
                ShellCommand::PanLeft
                    | ShellCommand::PanRight
                    | ShellCommand::PanUp
                    | ShellCommand::PanDown
                    | ShellCommand::PanTo { .. }
                    | ShellCommand::ZoomIn
                    | ShellCommand::ZoomOut
                    | ShellCommand::ZoomTo(_)
                    | ShellCommand::ZoomOnOutput { .. }
            )
        {
            return ControlReply::Err(
                "dismiss the window overlay before changing the canvas view".into(),
            );
        }

        match action {
            ShellCommand::CanvasTool(tool) => return action_reply(self.select_canvas_tool(tool)),
            ShellCommand::GroupSelectionToggle => {
                return action_reply(self.toggle_group_selection());
            }
            ShellCommand::GroupClear => {
                self.clear_window_group();
                return ControlReply::Ok;
            }
            ShellCommand::GroupGather => return action_reply(self.start_group_gather()),
            ShellCommand::GroupStatus => return ControlReply::Payload(self.group_status_json()),
            ShellCommand::LockSession => return action_reply(self.lock_shield()),
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
            ShellCommand::AcceptShellEvent(token) => {
                return action_reply(self.accept_shell_event(token));
            }
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
                return self.launch_app_reply(&app_id, None);
            }
            ShellCommand::ListApps => return ControlReply::Payload(self.list_apps_json()),
            ShellCommand::OpenSearchSettings { output_name } => {
                return self.open_search_settings_reply(output_name.as_deref());
            }
            ShellCommand::LaunchAppOnOutput {
                app_id,
                output_name,
            } => {
                return self.launch_app_reply(&app_id, Some(&output_name));
            }
            ShellCommand::PanLeft => self.pan_viewport_by(-self.horizontal_pan_step(), 0),
            ShellCommand::PanRight => self.pan_viewport_by(self.horizontal_pan_step(), 0),
            ShellCommand::PanUp => self.pan_viewport_by(0, -self.vertical_pan_step()),
            ShellCommand::PanDown => self.pan_viewport_by(0, self.vertical_pan_step()),
            ShellCommand::PanTo { x, y } => return action_reply(self.pan_viewport_to(x, y)),
            ShellCommand::ZoomIn => self.animate_zoom_around_viewport_center(ZOOM_STEP),
            ShellCommand::ZoomOut => self.animate_zoom_around_viewport_center(1.0 / ZOOM_STEP),
            ShellCommand::ZoomTo(scale) => return action_reply(self.zoom_viewport_to(scale)),
            ShellCommand::ZoomOnOutput {
                output_name,
                operation,
            } => {
                return action_reply(self.zoom_viewport_on_output(&output_name, operation));
            }
            ShellCommand::ReloadSettings => return self.settings_subscription.refresh(),
            ShellCommand::LogAccessibilityTree => {
                crate::accessibility::log_accessibility_tree(self.accessibility_window_snapshot())
            }
            // The shared keyboard path requests redraws for compositor actions;
            // ordinary client keys need no additional frame before their commit.
            ShellCommand::KeyDown(keycode) => {
                self.synthesize_key(keycode, KeyState::Pressed);
                return ControlReply::Ok;
            }
            ShellCommand::KeyUp(keycode) => {
                self.synthesize_key(keycode, KeyState::Released);
                return ControlReply::Ok;
            }
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
}

fn action_reply(result: Result<(), String>) -> ControlReply {
    result.map_or_else(ControlReply::Err, |_| ControlReply::Ok)
}
