//! X11 menus report root coordinates, while their owner may be projected onto
//! a panned canvas, an output, or a frame attachment. Preserve the client-local
//! offset and reuse the owner's transform for rendering and pointer input.
use smithay::utils::{Logical, Point};

use crate::compositor::{App, ManagedSurface};

impl App {
    pub(in crate::compositor) fn x11_popup_anchor(
        &self,
        index: usize,
    ) -> Option<(usize, Point<i32, Logical>)> {
        let ManagedSurface::X11(popup) = &self.windows[index].surface else {
            return None;
        };
        if !popup.is_override_redirect() {
            return None;
        }
        let origin = popup.last_configure().loc;
        let mut current = index;
        // Client-supplied transient chains can be cyclic or outlive their owner.
        for _ in 0..self.windows.len() {
            let parent = self.transient_parent_index(current)?;
            let ManagedSurface::X11(surface) = &self.windows[parent].surface else {
                return None;
            };
            if !surface.is_override_redirect() {
                return self.windows[parent]
                    .mapped
                    .then(|| (parent, origin - surface.last_configure().loc));
            }
            current = parent;
        }
        None
    }
}
