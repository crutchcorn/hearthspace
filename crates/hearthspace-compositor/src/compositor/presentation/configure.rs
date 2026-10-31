//! Client protocol geometry for canvas and output presentations.
use super::{App, ManagedSurface, set_state};
use crate::config::TITLE_BAR_HEIGHT;
#[cfg(feature = "xwayland")]
use smithay::utils::{Point, Rectangle};
use wayland_protocols::xdg::shell::server::xdg_toplevel;

impl App {
    pub(in crate::compositor) fn configure_window_presentation(&self, index: usize) {
        let window = &self.windows[index];
        let state = &window.presentation;
        let bounds = self
            .presentation_output_rect(index)
            .or_else(|| state.maximized.then_some(state.maximized_bounds).flatten());
        let chrome = if self.has_compositor_chrome(index) {
            TITLE_BAR_HEIGHT
        } else {
            0
        };
        let size = bounds
            .map(|rect| (rect.size.w.max(1), (rect.size.h - chrome).max(1)).into())
            .or(state.restore_size);
        match &window.surface {
            ManagedSurface::Xdg(surface) => {
                surface.with_pending_state(|pending| {
                    set_state(
                        &mut pending.states,
                        xdg_toplevel::State::Fullscreen,
                        state.fullscreen,
                    );
                    set_state(
                        &mut pending.states,
                        xdg_toplevel::State::Maximized,
                        state.maximized,
                    );
                    pending.states.unset(xdg_toplevel::State::Resizing);
                    pending.fullscreen_output = None;
                    pending.bounds = None;
                    pending.size = size;
                });
                if surface.is_initial_configure_sent() {
                    crate::compositor::gtk_shell::send_configure(surface);
                }
            }
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(surface) => {
                if let Err(error) = surface
                    .set_fullscreen(state.fullscreen)
                    .and_then(|()| surface.set_maximized(state.maximized))
                {
                    tracing::warn!(%error, "failed to update X11 window state");
                }
                if let Some(size) = size {
                    let origin = bounds
                        .map(|bounds| bounds.loc + Point::from((0, chrome)))
                        .unwrap_or_else(|| (window.position.x, window.position.y + chrome).into());
                    let geometry = self.x11_root_geometry(surface, Rectangle::new(origin, size));
                    if let Err(error) = surface.configure(Some(geometry)) {
                        tracing::warn!(%error, "failed to configure X11 window state");
                    }
                }
            }
        }
    }
}
