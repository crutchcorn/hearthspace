use smithay::{
    backend::renderer::utils::with_renderer_surface_state, desktop::find_popup_root_surface,
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::protocol::wl_surface::WlSurface;

use super::super::{App, ManagedSurface};

impl App {
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
        self.windows[index].mapped = mapped;
        let id = self.windows[index].id;
        if mapped && self.is_shell_panel(index) {
            // Only the initial mapping requests focus; zoom updates and other
            // buffer commits must not steal it back from a newer user action.
            if self.navigation_available() && self.search_mapping_allowed(index) {
                self.set_keyboard_focus_to_window(index, root.clone());
                self.finish_pending_search(index);
            } else {
                self.windows[index].surface.send_close();
            }
        } else if mapped && self.is_navigable_window(index) {
            self.position_transient(index);
            if !self.windows[index].initial_placement_done {
                self.windows[index].initial_placement_done = true;
                self.register_window_placement(index);
                self.center_new_window(index);
            }
            // An existing popup/drag owns input until it ends. A background app
            // mapping during that interaction must not break its grab.
            if self.navigation_available() {
                // Mapping grants keyboard focus, but never navigates the canvas.
                // Explicit selection (Alt-Tab, search, agent focus) still reveals.
                let index = self.raise_window(index);
                self.set_keyboard_focus_to_window(index, root.clone());
            }
        } else if !mapped && self.focused_normal_window_id == Some(id) {
            self.clear_keyboard_focus_for_surface(root);
            self.restore_window_focus();
        }
    }

    pub(in crate::compositor) fn sync_window_activation(&mut self, focus: Option<&WlSurface>) {
        let index = focus.and_then(|surface| {
            self.window_index_for_surface(surface).or_else(|| {
                self.popups
                    .find_popup(surface)
                    .and_then(|popup| find_popup_root_surface(&popup).ok())
                    .and_then(|root| self.window_index_for_surface(&root))
            })
        });
        self.cancel_search_outside_focus(index);
        self.dismiss_shell_panels_outside_focus(index);
        self.focused_normal_window_id = index
            .filter(|index| self.is_navigable_window(*index))
            .map(|index| self.windows[index].id);
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
                        surface.send_configure();
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
            let _ = self.focus_window(id);
        } else {
            self.clear_keyboard_focus();
        }
    }
}
