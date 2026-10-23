//! X11 menus report root coordinates, while their owner may be projected onto
//! a panned canvas, an output, or a frame attachment. Preserve the client-local
//! offset and reuse the owner's transform for rendering and pointer input.
use smithay::utils::{Logical, Point, Rectangle};

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

    pub(in crate::compositor) fn x11_popup_screen_origin(
        &self,
        index: usize,
    ) -> Option<Point<f64, Logical>> {
        let (parent, offset) = self.x11_popup_anchor(index)?;
        let origin = self.window_surface_screen_origin(parent);
        let scale = self.window_render_scale(parent);
        Some(
            (
                origin.x + f64::from(offset.x) * scale,
                origin.y + f64::from(offset.y) * scale,
            )
                .into(),
        )
    }

    pub(in crate::compositor) fn x11_popup_screen_bounds(
        &self,
        index: usize,
    ) -> Option<Rectangle<i32, Logical>> {
        let (parent, _) = self.x11_popup_anchor(index)?;
        let scale = self.window_render_scale(parent);
        let origin = self.x11_popup_screen_origin(index)?;
        let inset = self.window_surface_geometry_loc(index);
        let size = self.window_geometry_size(index);
        Some(Rectangle::new(
            (
                (origin.x + f64::from(inset.x) * scale).round() as i32,
                (origin.y + f64::from(inset.y) * scale).round() as i32,
            )
                .into(),
            (
                (f64::from(size.w) * scale).round() as i32,
                (f64::from(size.h) * scale).round() as i32,
            )
                .into(),
        ))
    }
}
