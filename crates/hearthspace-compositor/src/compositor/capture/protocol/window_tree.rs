//! The same selected surface trees supply pixels and their native capture density.
use smithay::{
    backend::renderer::utils::RendererSurfaceStateUserData,
    desktop::{PopupManager, utils::bbox_from_surface_tree},
    utils::{Logical, Point, Rectangle, Scale},
    wayland::{
        compositor::{TraversalAction, with_states, with_surface_tree_downward},
        shell::xdg::SurfaceCachedState,
    },
};
use wayland_server::protocol::wl_surface::WlSurface;

use super::App;

pub(super) fn effective_geometry(root: &WlSurface) -> Rectangle<i32, Logical> {
    let bbox = bbox_from_surface_tree(root, (0, 0));
    // Match Smithay desktop::Window::geometry without maintaining a second Window.
    with_states(root, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
            .and_then(|geometry| geometry.intersection(bbox))
    })
    .unwrap_or(bbox)
}

impl App {
    /// Front-to-back content roots and their positions in the selected root's space.
    pub(super) fn capture_window_trees(
        &self,
        index: usize,
        geometry_loc: Point<i32, Logical>,
    ) -> Vec<(WlSurface, Point<i32, Logical>)> {
        let Some(root) = self.windows[index].surface.wl_surface() else {
            return Vec::new();
        };
        let mut trees: Vec<_> = PopupManager::popups_for_surface(&root)
            .map(|(popup, offset)| {
                (
                    popup.wl_surface().clone(),
                    geometry_loc + offset - popup.geometry().loc,
                )
            })
            .collect();
        // Only explicitly owned override-redirect menus join an X11 source.
        // Other normal toplevels, including same-app dialogs, remain separate.
        #[cfg(feature = "xwayland")]
        for popup_index in (0..self.windows.len()).rev() {
            if let Some((parent, offset)) = self.x11_popup_anchor(popup_index)
                && parent == index
                && self.windows[popup_index].mapped
                && let Some(surface) = self.windows[popup_index].surface.wl_surface()
            {
                trees.push((surface, offset));
            }
        }
        trees.push((root, (0, 0).into()));
        trees
    }
}

pub(super) fn backing_scale(
    trees: &[(WlSurface, Point<i32, Logical>)],
    crop: Rectangle<i32, Logical>,
) -> Scale<f64> {
    // A stretched solid-color root does not describe the resolution of the
    // content. Keep at least one sample per logical pixel, then preserve the
    // highest backing density of every mapped tree intersecting the crop.
    let mut scale = Scale::from(1.0_f64);
    for (root, origin) in trees {
        with_surface_tree_downward(
            root,
            *origin,
            |_, states, origin| {
                let Some(data) = states.data_map.get::<RendererSurfaceStateUserData>() else {
                    return TraversalAction::SkipChildren;
                };
                let state = data.lock().unwrap();
                let Some(view) = state.view() else {
                    return TraversalAction::SkipChildren;
                };
                let origin = *origin + view.offset;
                if Rectangle::new(origin, view.dst)
                    .intersection(crop)
                    .is_some()
                    && view.dst.w > 0
                    && view.dst.h > 0
                {
                    // SurfaceView.src already uses transformed logical axes. Do
                    // not swap them again for rotated/flipped buffer transforms.
                    let x =
                        f64::from(state.buffer_scale()) * view.src.size.w / f64::from(view.dst.w);
                    let y =
                        f64::from(state.buffer_scale()) * view.src.size.h / f64::from(view.dst.h);
                    if x.is_finite() {
                        scale.x = scale.x.max(x);
                    }
                    if y.is_finite() {
                        scale.y = scale.y.max(y);
                    }
                }
                TraversalAction::DoChildren(origin)
            },
            |_, _, _| {},
            |_, _, _| true,
        );
    }
    scale
}
