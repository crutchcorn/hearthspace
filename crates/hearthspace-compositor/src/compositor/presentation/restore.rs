//! A restore configure can commit before the titlebar press is released.
use super::{App, ManagedSurface, xdg_toplevel};

impl App {
    pub(in crate::compositor) fn finish_presentation_restore(
        &mut self,
        root: &wayland_server::protocol::wl_surface::WlSurface,
    ) {
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.surface.wl_surface().as_ref() == Some(root))
        else {
            return;
        };
        if !self.windows[index].presentation.restoring {
            return;
        }
        let size = self.window_geometry_size(index);
        let ready = match &self.windows[index].surface {
            ManagedSurface::Xdg(surface) => surface.with_committed_state(|state| {
                state.is_some_and(|state| {
                    !state.states.contains(xdg_toplevel::State::Fullscreen)
                        && !state.states.contains(xdg_toplevel::State::Maximized)
                })
            }),
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(_) => self.windows[index].presentation.restore_size == Some(size),
        };
        if ready && size.w > 0 && size.h > 0 {
            self.windows[index].presentation.restoring = false;
            self.windows[index].presentation.restore_size = None;
            self.finish_pending_presentation_reveals();
        }
    }

    pub(in crate::compositor) fn finish_pending_presentation_reveals(&mut self) {
        for index in 0..self.windows.len() {
            let window = &self.windows[index];
            if !window.presentation.reveal_on_restore {
                continue;
            }
            let id = window.id;
            if !window.mapped
                || window.presentation.pinned()
                || self.focused_normal_window_id != Some(id)
            {
                self.windows[index].presentation.reveal_on_restore = false;
                continue;
            }
            if window.presentation.restoring || !self.navigation_available() {
                continue;
            }
            // A successful reveal consumes this request. Never bypass a new
            // grab or reclaim focus after the user selects another window.
            if self.focus_window(id).is_ok()
                && let Some(window) = self.windows.iter_mut().find(|window| window.id == id)
            {
                window.presentation.reveal_on_restore = false;
            }
        }
    }

    pub(in crate::compositor) fn cancel_presentation_reveals(&mut self) {
        for window in &mut self.windows {
            window.presentation.reveal_on_restore = false;
        }
    }
}
