//! A shell-owned toplevel palette with popup-like focus and dismissal policy.
//!
//! Xilem's current WindowOptions exposes toplevel windows, not xdg_popup. Keep
//! this palette in the existing shell layer while using standard keyboard
//! focus and xdg_toplevel.close events for its lifetime.
use hearthspace_common::canvas_map_output_name_from_app_id;
use serde_json::{Value, json};

use super::window_app_id;
use crate::compositor::{App, HitTarget, ManagedWindowKind};

impl App {
    pub(in crate::compositor) fn restore_focus_after_canvas_map(&mut self) {
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

    pub(in crate::compositor) fn is_canvas_map(&self, index: usize) -> bool {
        window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(canvas_map_output_name_from_app_id)
            .is_some()
    }

    pub(in crate::compositor) fn focused_canvas_map_index(&self) -> Option<usize> {
        let focus = self.keyboard.current_focus()?;
        let index = self.window_index_for_surface(focus.surface())?;
        self.is_canvas_map(index).then_some(index)
    }

    pub(in crate::compositor) fn dismiss_focused_canvas_map(&self) {
        if let Some(index) = self.focused_canvas_map_index() {
            self.windows[index].surface.send_close();
        }
    }

    /// The map's own bar belongs to its focus group so pressing its toggle
    /// cannot close on press and immediately reopen on release. Other bar
    /// actions dismiss it in the shell before running their callback.
    pub(in crate::compositor) fn dismiss_canvas_maps_outside_focus(&self, focus: Option<usize>) {
        for (index, window) in self.windows.iter().enumerate() {
            if !window.mapped || !self.is_canvas_map(index) {
                continue;
            }
            let keep = focus.is_some_and(|focus| {
                if focus == index {
                    return true;
                }
                let target = &self.windows[focus];
                target.kind == ManagedWindowKind::ShellBar
                    && self
                        .output_logical_rect_for_name(window.shell_output_name.as_deref())
                        .is_some_and(|output| {
                            self.output_logical_rect_for_name(target.shell_output_name.as_deref())
                                == Some(output)
                        })
            });
            if !keep {
                window.surface.send_close();
            }
        }
    }

    pub(in crate::compositor) fn dismiss_canvas_maps_outside_pointer(&self) {
        let focus = self
            .hit_test(self.pointer_location)
            .map(|target| match target {
                HitTarget::Client { window_index, .. }
                | HitTarget::CloseButton { window_index }
                | HitTarget::TitleBar { window_index }
                | HitTarget::ResizeBorder { window_index, .. } => window_index,
            });
        self.dismiss_canvas_maps_outside_focus(focus);
    }

    pub(in crate::compositor) fn canvas_maps_status(&self) -> Vec<Value> {
        self.windows
            .iter()
            .enumerate()
            .filter(|(index, window)| window.mapped && self.is_canvas_map(*index))
            .filter_map(|(index, window)| {
                let output =
                    self.output_logical_rect_for_name(window.shell_output_name.as_deref())?;
                if !self.shell_visible_on_output(output) {
                    return None;
                }
                let origin =
                    self.launcher_logical_origin(index)? + self.window_surface_geometry_loc(index);
                let size = self.window_geometry_size(index);
                Some(json!({
                    "output": window.shell_output_name,
                    "x": origin.x, "y": origin.y,
                    "width": size.w, "height": size.h,
                }))
            })
            .collect()
    }
}
