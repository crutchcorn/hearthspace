use smithay::{
    backend::renderer::utils::with_renderer_surface_state, desktop::find_popup_root_surface,
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::protocol::wl_surface::WlSurface;

use super::super::{App, ManagedSurface};

impl App {
    /// Unmapping one window must not end another window's interaction. Keep
    /// the input sequence alive so its eventual release still reaches the grab.
    pub(in crate::compositor) fn retire_window_pointer_operation(&mut self, id: u64) {
        if self.drag.as_ref().is_some_and(|drag| drag.window_id == id) {
            self.drag = None;
        }
        if self
            .resize
            .as_ref()
            .is_some_and(|resize| resize.window_id == id)
        {
            self.resize = None;
        }
        self.retire_window_ui_touch(id);
    }

    /// Mapping follows the root buffer, not subsurface commits or creation of
    /// an empty toplevel. Unmapping and later remapping starts a new focus cycle.
    pub(in crate::compositor) fn refresh_window_mapping(&mut self, root: &WlSurface) {
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.surface.wl_surface().as_ref() == Some(root))
        else {
            return;
        };
        let mapped =
            with_renderer_surface_state(root, |state| state.buffer().is_some()).unwrap_or(false);
        if mapped == self.windows[index].mapped {
            return;
        }
        let id = self.windows[index].id;
        if !mapped {
            self.retire_window(id, super::super::windows::RetirementCause::Unmapped);
            return;
        }
        self.windows[index].mapped = true;
        self.sync_capture_toplevel(id);
        if self.windows[index].kind == super::super::ManagedWindowKind::DesktopItem {
            self.desktop_mapped(index);
        }
        if self.reserves_shell_work_area(index) {
            self.reconcile_window_presentations();
        }
        let Some(index) = self.windows.index(id) else {
            return;
        };
        if self.is_power_confirmation(index) {
            self.power_confirmation_mapped(index);
        } else if self.is_layout_surface(index) {
            self.layout_surface_mapped(index);
        } else if self.is_window_switcher_caption(index) {
            if !self.window_switcher_caption_visible(index) {
                self.windows[index].surface.send_close();
            }
        } else if self.windows[index].kind == super::super::ManagedWindowKind::ControlOsd {
            if self.session_locked() {
                self.windows[index].surface.send_close();
            }
        } else if self.is_shell_panel(index) {
            // Only the initial mapping requests focus; zoom updates and other
            // buffer commits must not steal it back from a newer user action.
            if (self.navigation_available() || self.canvas_search_panel_allowed(index))
                && self.desktop_context_mapping_allowed(index)
                && self.search_mapping_allowed(index)
            {
                self.set_keyboard_focus_to_window(index, root.clone());
                if let Some(index) = self.windows.index(id) {
                    self.finish_pending_search(index);
                }
            } else {
                self.windows[index].surface.send_close();
            }
        } else if self.is_navigable_window(index) {
            self.position_transient(index);
            if self
                .windows
                .by_id(id)
                .is_some_and(|window| !window.initial_placement_done)
            {
                // Restoring saved placement can reconcile the layout. Keep
                // this window out of canvas settlement until its final first-
                // map position is known, rather than exposing the origin.
                let Some(index) = self.windows.index(id) else {
                    return;
                };
                self.register_window_placement(index);
                let Some(index) = self.windows.index(id) else {
                    return;
                };
                self.center_new_window(index);
                if let Some(window) = self.windows.by_id_mut(id) {
                    window.initial_placement_done = true;
                }
            }
            // An existing popup/drag owns input until it ends. A background app
            // mapping during that interaction must not break its grab.
            if self.navigation_available() {
                // Mapping grants keyboard focus, but never navigates the canvas.
                // Explicit selection (Alt-Tab, search, agent focus) still reveals.
                if let Some(index) = self.raise_window_id(id) {
                    self.set_keyboard_focus_to_window(index, root.clone());
                }
            }
        }
    }

    pub(in crate::compositor) fn sync_window_activation(&mut self, focus: Option<&WlSurface>) {
        let id = focus
            .and_then(|surface| {
                self.window_index_for_surface(surface).or_else(|| {
                    self.popups
                        .find_popup(surface)
                        .and_then(|popup| find_popup_root_surface(&popup).ok())
                        .and_then(|root| self.window_index_for_surface(&root))
                })
            })
            .map(|index| self.windows[index].id);
        self.window_focus_view_changed(id.and_then(|id| self.windows.index(id)));
        self.desktop_focus_changed(id.and_then(|id| self.windows.index(id)));
        self.layout_focus_changed(id.and_then(|id| self.windows.index(id)));
        self.cancel_search_outside_focus(id.and_then(|id| self.windows.index(id)));
        self.favorite_focus_changed(id);
        self.ui_touch_focus_changed(id, focus);
        self.dismiss_shell_panels_outside_focus(id.and_then(|id| self.windows.index(id)));
        self.focused_normal_window_id = id.filter(|id| {
            self.windows
                .index(*id)
                .is_some_and(|index| self.is_navigable_window(index))
        });
        for (index, window) in self.windows.iter().enumerate() {
            if !self.is_navigable_window(index) {
                continue;
            }
            let active = self.focused_normal_window_id == Some(window.id);
            match &window.surface {
                ManagedSurface::Xdg(surface) => {
                    let changed = surface.with_pending_state(|state| {
                        let changed =
                            state.states.contains(xdg_toplevel::State::Activated) != active;
                        if active {
                            state.states.set(xdg_toplevel::State::Activated);
                        } else {
                            state.states.unset(xdg_toplevel::State::Activated);
                        }
                        changed
                    });
                    if changed {
                        crate::compositor::gtk_shell::send_configure(surface);
                    }
                }
                #[cfg(feature = "xwayland")]
                ManagedSurface::X11(surface) => {
                    if let Err(error) = surface.set_activated(active) {
                        tracing::warn!(%error, "failed to update X11 window activation");
                    }
                }
            }
        }
        self.request_redraw();
    }

    pub(in crate::compositor) fn restore_window_focus(&mut self) {
        if !self.navigation_available() {
            return;
        }
        let target = self
            .windows
            .iter()
            .enumerate()
            .rev()
            .find(|(index, _)| self.is_navigable_window(*index))
            .map(|(_, window)| window.id);
        if let Some(id) = target {
            // Closing or unmapping a window hands off keyboard focus without
            // navigating to an offscreen successor. Preserve the current view
            // and any user-owned viewport animation during automatic recovery.
            let Some(index) = self.raise_window_id(id) else {
                return;
            };
            if let Some(surface) = self.windows[index].surface.wl_surface() {
                self.set_keyboard_focus_to_window(index, surface);
            }
        } else {
            self.clear_keyboard_focus();
        }
    }
}
