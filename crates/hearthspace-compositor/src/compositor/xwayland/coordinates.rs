//! X11 has a bounded root desktop; the canvas is a separate coordinate space.
use smithay::{
    utils::{Logical, Rectangle},
    xwayland::X11Surface,
};

use crate::compositor::App;

impl App {
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
