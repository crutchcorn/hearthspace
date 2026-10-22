//! A shell-owned toplevel palette with popup-like focus and dismissal policy.
//!
//! Xilem's current WindowOptions exposes toplevel windows, not xdg_popup. Keep
//! this palette in the existing shell layer while using standard keyboard
//! focus and xdg_toplevel.close events for its lifetime.
use hearthspace_common::{
    canvas_map_output_name_from_app_id, launcher_output_name_from_app_id,
    notifications_output_name_from_app_id, power_menu_output_name_from_app_id,
    quick_panel_output_name_from_app_id, tray_menu_output_name_from_app_id,
};
use serde_json::{Value, json};
use smithay::utils::{Logical, Point};

use super::window_app_id;
use crate::compositor::{App, HitTarget, ManagedWindowKind};

impl App {
    pub(in crate::compositor) fn restore_focus_after_shell_panel(&mut self) {
        if !self.navigation_available() {
            return;
        }
        let index = self
            .windows
            .iter()
            .enumerate()
            .rev()
            .find(|(index, _)| self.is_navigable_window(*index))
            .map(|(index, _)| index);
        if let Some(index) = index
            && let Some(surface) = self.windows[index].surface.wl_surface()
        {
            // Closing view controls must retain the view the user just chose,
            // even if the restored keyboard target is now outside that view.
            self.set_keyboard_focus_to_window(index, surface);
        }
    }

    pub(in crate::compositor) fn is_shell_panel(&self, index: usize) -> bool {
        self.windows[index].kind == ManagedWindowKind::Launcher
            && self.canvas_search_result_identity(index).is_none()
            && !self.is_window_switcher_caption(index)
    }

    pub(in crate::compositor) fn is_search_panel(&self, index: usize) -> bool {
        window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(launcher_output_name_from_app_id)
            .is_some()
    }

    pub(in crate::compositor) fn focused_shell_panel_index(&self) -> Option<usize> {
        let focus = self.keyboard.current_focus()?;
        let index = self.window_index_for_surface(focus.surface())?;
        self.is_shell_panel(index).then_some(index)
    }

    /// A palette's own bar keeps the palette alive until its icon callback runs.
    /// Escape and restoration on close must recognize the same focus group.
    pub(in crate::compositor) fn active_shell_panel_index(&self) -> Option<usize> {
        let focus = self.keyboard.current_focus()?;
        let focus = self.window_index_for_surface(focus.surface())?;
        if self.is_shell_panel(focus) {
            return Some(focus);
        }
        self.windows
            .iter()
            .enumerate()
            .rev()
            .find(|(index, window)| {
                window.mapped
                    && self.is_shell_panel(*index)
                    && self.panel_keeps_focus(*index, focus)
            })
            .map(|(index, _)| index)
    }

    fn panel_keeps_focus(&self, panel: usize, focus: usize) -> bool {
        if focus == panel {
            return true;
        }
        let target = &self.windows[focus];
        target.kind == ManagedWindowKind::ShellBar
            && self
                .output_logical_rect_for_name(self.windows[panel].shell_output_name.as_deref())
                .is_some_and(|output| {
                    self.output_logical_rect_for_name(target.shell_output_name.as_deref())
                        == Some(output)
                })
    }

    pub(in crate::compositor) fn dismiss_active_shell_panel(&self) {
        if let Some(index) = self.active_shell_panel_index() {
            self.windows[index].surface.send_close();
        }
    }

    /// The map's own bar belongs to its focus group so pressing its toggle
    /// cannot close on press and immediately reopen on release. Other bar
    /// actions dismiss it in the shell before running their callback.
    pub(in crate::compositor) fn dismiss_shell_panels_outside_focus(&self, focus: Option<usize>) {
        for (index, window) in self.windows.iter().enumerate() {
            if !window.mapped || !self.is_shell_panel(index) {
                continue;
            }
            let keep = (self.canvas_search_active() && self.canvas_search_panel_allowed(index))
                || focus.is_some_and(|focus| self.panel_keeps_focus(index, focus));
            if !keep {
                window.surface.send_close();
            }
        }
    }

    pub(in crate::compositor) fn dismiss_shell_panels_outside_pointer(&self) {
        let focus = self
            .hit_test(self.pointer_location)
            .map(|target| match target {
                HitTarget::Client { window_index, .. }
                | HitTarget::TitlebarButton { window_index, .. }
                | HitTarget::TitleBar { window_index }
                | HitTarget::ResizeBorder { window_index, .. } => window_index,
            });
        self.dismiss_shell_panels_outside_focus(focus);
    }

    /// Real outside presses also revoke openings which have not mapped yet.
    /// Focus callbacks cannot do this: mapping/restoring shell windows also
    /// changes focus, and the initiating bar belongs to the popup's input group.
    pub(in crate::compositor) fn cancel_shell_popup_outside_press(
        &mut self,
        location: Point<f64, Logical>,
    ) {
        if !self.navigation_available() {
            return;
        }
        let target = self.hit_test(location).map(|target| match target {
            HitTarget::Client { window_index, .. }
            | HitTarget::TitlebarButton { window_index, .. }
            | HitTarget::TitleBar { window_index }
            | HitTarget::ResizeBorder { window_index, .. } => window_index,
        });
        if target.is_none_or(|index| {
            self.windows[index].kind != ManagedWindowKind::ShellBar && !self.is_shell_panel(index)
        }) {
            self.invalidate_shell_popups();
        }
    }

    pub(in crate::compositor) fn canvas_maps_status(&self) -> Vec<Value> {
        self.shell_panels_status()
            .into_iter()
            .filter(|panel| panel["kind"] == "map")
            .map(|mut panel| {
                panel.as_object_mut().unwrap().remove("kind");
                panel
            })
            .collect()
    }

    pub(in crate::compositor) fn shell_panels_status(&self) -> Vec<Value> {
        self.windows
            .iter()
            .enumerate()
            .filter(|(index, window)| window.mapped && self.is_shell_panel(*index))
            .filter_map(|(index, window)| {
                let output =
                    self.output_logical_rect_for_name(window.shell_output_name.as_deref())?;
                if !self.shell_visible_on_output(output) {
                    return None;
                }
                let origin =
                    self.launcher_logical_origin(index)? + self.window_surface_geometry_loc(index);
                let size = self.window_geometry_size(index);
                let app_id = window_app_id(&window.surface)?;
                let kind = if hearthspace_common::desktop_context::identity(&app_id).is_some() {
                    "desktop-context"
                } else if canvas_map_output_name_from_app_id(&app_id).is_some() {
                    "map"
                } else if launcher_output_name_from_app_id(&app_id).is_some() {
                    "search"
                } else if notifications_output_name_from_app_id(&app_id).is_some() {
                    "notifications"
                } else if power_menu_output_name_from_app_id(&app_id).is_some() {
                    "power"
                } else if quick_panel_output_name_from_app_id(&app_id).is_some() {
                    "controls"
                } else if tray_menu_output_name_from_app_id(&app_id).is_some() {
                    "tray"
                } else {
                    return None;
                };
                Some(json!({
                    "kind": kind,
                    "output": window.shell_output_name,
                    "x": origin.x, "y": origin.y,
                    "width": size.w, "height": size.h,
                }))
            })
            .collect()
    }
}
