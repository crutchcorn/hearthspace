//! Execute intercepted shortcuts after Smithay releases its keyboard mutex.
use super::Shortcut;
use crate::compositor::{App, shell_integration::ControlReply, switcher::SwitcherSource};
use hearthspace_ipc::command::{ShellCommand, SpawnTarget};

impl App {
    pub(super) fn run_shortcut(&mut self, shortcut: Shortcut) {
        match shortcut {
            Shortcut::CanvasPan => {
                let _ = self.select_canvas_tool(hearthspace_ipc::command::CanvasTool::Pan);
                return;
            }
            Shortcut::CanvasPanCancel => {
                self.cancel_canvas_pan();
                return;
            }
            Shortcut::CanvasSearch(action) => {
                self.run_canvas_search_key(action);
                return;
            }
            Shortcut::Desktop(action) => {
                self.run_desktop_key(action);
                return;
            }
            Shortcut::Group(action) => {
                self.run_group_shortcut(action);
                return;
            }
            Shortcut::DeviceControl(action) => {
                if let Err(error) = self.send_device_control(action) {
                    tracing::debug!(%error, "device control unavailable");
                }
                return;
            }
            Shortcut::Next | Shortcut::Previous => {
                if let Err(error) = self
                    .cycle_window_switcher(SwitcherSource::Keyboard, shortcut == Shortcut::Previous)
                {
                    tracing::debug!(%error, "window switching unavailable");
                }
                return;
            }
            Shortcut::SwitcherCommit => {
                self.dismiss_window_switcher(false);
                return;
            }
            Shortcut::SwitcherCancel => {
                self.dismiss_window_switcher(true);
                return;
            }
            Shortcut::ShellPanelDismiss => {
                self.invalidate_shell_popups();
                // Revoke before the asynchronous close reaches the shell: a
                // held drawing button may be released in the same input batch.
                self.clear_favorite_drawing();
                self.cancel_ui_touch();
                if self.active_shell_panel_index().is_some() {
                    self.dismiss_active_shell_panel();
                } else {
                    self.restore_focus_after_shell_panel();
                }
                return;
            }
            Shortcut::ShellCornerCycle { reverse } => {
                self.cycle_shell_corner(reverse);
                return;
            }
            _ => {}
        }
        if matches!(shortcut, Shortcut::FocusedView | Shortcut::Maximize) {
            if let Some(id) = self.focused_normal_window_id {
                let result = if shortcut == Shortcut::FocusedView {
                    self.toggle_window_focus_view(id)
                } else {
                    self.toggle_window_canvas_maximize(id)
                };
                if let Err(error) = result {
                    tracing::debug!(%error, "window presentation shortcut unavailable");
                }
            }
            return;
        }
        let command = match shortcut {
            Shortcut::DeviceControl(_)
            | Shortcut::CanvasPan
            | Shortcut::CanvasPanCancel
            | Shortcut::CanvasSearch(_)
            | Shortcut::Desktop(_)
            | Shortcut::Group(_)
            | Shortcut::SwitcherCommit
            | Shortcut::SwitcherCancel
            | Shortcut::ShellPanelDismiss
            | Shortcut::ShellCornerCycle { .. }
            | Shortcut::Next
            | Shortcut::FocusedView
            | Shortcut::Maximize
            | Shortcut::Previous => unreachable!(),
            Shortcut::Lock => ShellCommand::LockSession,
            Shortcut::Terminal => ShellCommand::Spawn(SpawnTarget::Foot),
            Shortcut::Launcher => ShellCommand::FocusLauncher,
            Shortcut::Close => {
                let Some(id) = self.focused_normal_window_id else {
                    return;
                };
                ShellCommand::CloseWindow(id)
            }
            Shortcut::Left => ShellCommand::PanLeft,
            Shortcut::Right => ShellCommand::PanRight,
            Shortcut::Up => ShellCommand::PanUp,
            Shortcut::Down => ShellCommand::PanDown,
            Shortcut::Fullscreen => {
                let Some(id) = self.focused_normal_window_id else {
                    return;
                };
                let Some(window) = self.windows.iter().find(|window| window.id == id) else {
                    return;
                };
                ShellCommand::FullscreenWindow {
                    id,
                    enabled: !window.presentation.fullscreen,
                }
            }
        };
        if let ControlReply::Err(error) = self.run_control_action(command) {
            tracing::warn!(%error, ?shortcut, "keyboard action failed");
        }
    }
}
