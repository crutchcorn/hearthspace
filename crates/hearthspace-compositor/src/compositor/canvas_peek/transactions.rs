//! Only committed placement geometry may displace neighboring canvas objects.
use smithay::wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData};
use wayland_protocols::xdg::shell::server::xdg_toplevel;

use crate::compositor::{App, ManagedSurface};

impl App {
    pub(in crate::compositor) fn canvas_peek_transaction_pending(&self) -> bool {
        self.resize.is_some()
            || self.windows.iter().any(|window| {
                if !window.mapped
                    || (window.pending_resize.is_none() && !window.presentation.maximized)
                {
                    return false;
                }
                match &window.surface {
                    ManagedSurface::Xdg(surface) => {
                        let configured = with_states(surface.wl_surface(), |states| {
                            states
                                .data_map
                                .get::<XdgToplevelSurfaceData>()
                                .unwrap()
                                .lock()
                                .unwrap()
                                .current_server_state()
                        });
                        // The release configure can change only Resizing. Its
                        // anchor may linger until acknowledgment even though
                        // the final geometry was already committed while held.
                        // Conversely an ack alone must not expose the old size:
                        // Smithay's committed state advances with the buffer.
                        surface.with_committed_state(|state| {
                            state.and_then(|state| state.size) != configured.size
                                // Maximization relocates the origin before the
                                // client commits, even if its size is unchanged.
                                // Compare applied configures, not buffer size:
                                // clients can legitimately constrain that size.
                                || (window.presentation.maximized
                                    && state.is_none_or(|state| {
                                        state.states.contains(xdg_toplevel::State::Maximized)
                                            != configured
                                                .states
                                                .contains(xdg_toplevel::State::Maximized)
                                    }))
                        })
                    }
                    // X11 has no configure serial. This is the same committed
                    // buffer geometry used to retire its final resize anchor.
                    #[cfg(feature = "xwayland")]
                    ManagedSurface::X11(surface) => {
                        surface.bbox().size != surface.last_configure().size
                    }
                }
            })
    }
}
