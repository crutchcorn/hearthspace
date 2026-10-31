//! X11 has a bounded root desktop; the canvas is a separate coordinate space.
use smithay::{
    utils::{Logical, Rectangle},
    xwayland::X11Surface,
};

use crate::compositor::{App, ManagedSurface};
use wayland_server::protocol::wl_surface::WlSurface;

impl App {
    pub(in crate::compositor) fn prepare_x11_pointer_target(&mut self, target: &WlSurface) {
        // Canvas windows may share X root coordinates. XWayland still uses
        // X stacking for delivery, so the compositor's hit target must be on
        // top there before sending surface-local pointer input. An existing
        // grab retains its original target until all buttons are released.
        if self.pointer.is_grabbed() {
            return;
        }
        let Some(index) = self.window_index_for_surface(target) else {
            return;
        };
        let ManagedSurface::X11(surface) = &self.windows[index].surface else {
            return;
        };
        if let Some(xwm) = &mut self.xwm
            && let Err(error) = xwm.raise_window(surface.as_ref())
        {
            tracing::warn!(%error, "failed to raise X11 pointer target");
        }
    }

    pub(in crate::compositor) fn x11_root_geometry(
        &self,
        surface: &X11Surface,
        mut geometry: Rectangle<i32, Logical>,
    ) -> Rectangle<i32, Logical> {
        // Keep the backing origin stable for the lifetime of a managed X11
        // window. Canvas placement must neither clip X input at the root's
        // edges nor change the root-relative offsets of already mapped menus.
        // Override-redirect windows retain the client's own root coordinates.
        if !surface.is_override_redirect() {
            geometry.loc = (0, 0).into();
        }
        geometry
    }
}
