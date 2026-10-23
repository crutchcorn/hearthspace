//! Only decorate a CSD rectangle when its coverage is established by the client.
use smithay::{
    backend::renderer::{buffer_has_alpha, utils::RendererSurfaceStateUserData},
    utils::{Logical, Rectangle, Size},
    wayland::{
        alpha_modifier::AlphaModifierSurfaceCachedState,
        compositor::{
            RectangleKind, RegionAttributes, SurfaceAttributes, TraversalAction,
            with_surface_tree_downward,
        },
    },
};
use wayland_server::protocol::wl_surface::WlSurface;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::compositor) enum FrameKind {
    ServerSide,
    RectangularClient,
    UncertainClient,
}

impl FrameKind {
    pub fn for_client(root: &WlSurface, window: Rectangle<i32, Logical>) -> Self {
        let mut kind = Self::UncertainClient;
        with_surface_tree_downward(
            root,
            (),
            |surface, states, &()| {
                let Some(data) = states.data_map.get::<RendererSurfaceStateUserData>() else {
                    return TraversalAction::SkipChildren;
                };
                let data = data.lock().unwrap();
                let (Some(view), Some(buffer)) = (data.view(), data.buffer()) else {
                    return TraversalAction::SkipChildren;
                };
                // Match Smithay's mapped-tree traversal. A separate popup is
                // not part of this tree and does not change its owner's shape.
                if surface != root {
                    kind = Self::UncertainClient;
                    return TraversalAction::Break;
                }
                let mut attrs = states.cached_state.get::<SurfaceAttributes>();
                let mut alpha = states.cached_state.get::<AlphaModifierSurfaceCachedState>();
                kind = for_coverage(
                    view.dst,
                    window,
                    buffer_has_alpha(buffer),
                    attrs.current().opaque_region.as_ref(),
                    alpha.current().multiplier(),
                );
                if kind == Self::RectangularClient {
                    TraversalAction::DoChildren(())
                } else {
                    TraversalAction::Break
                }
            },
            |_, _, &()| {},
            |_, _, &()| true,
        );
        kind
    }
}

pub(super) fn for_coverage(
    surface_size: Size<i32, Logical>,
    window: Rectangle<i32, Logical>,
    has_alpha: Option<bool>,
    opaque: Option<&RegionAttributes>,
    alpha: Option<u32>,
) -> FrameKind {
    // Surface coordinates already account for buffer scale, transform and
    // viewporter cropping. Canvas zoom must not change this coverage decision.
    if window.is_empty()
        || !Rectangle::from_size(surface_size).contains_rect(window)
        || alpha.is_some_and(|alpha| alpha != u32::MAX)
    {
        return FrameKind::UncertainClient;
    }
    if has_alpha == Some(false) || opaque.is_some_and(|region| covers(region, window)) {
        FrameKind::RectangularClient
    } else {
        FrameKind::UncertainClient
    }
}

fn covers(region: &RegionAttributes, window: Rectangle<i32, Logical>) -> bool {
    // Omission is safe for complex hints. Bound both protocol operations and
    // rectangle fragmentation rather than doing unbounded work per frame.
    const MAX_RECTS: usize = 64;
    if region.rects.len() > MAX_RECTS {
        return false;
    }
    let mut uncovered = vec![window];
    // The last operation affecting a pixel determines its membership. Walking
    // backwards lets us prove unions without treating a corner cutout as opaque.
    for (kind, rect) in region.rects.iter().rev() {
        match kind {
            RectangleKind::Add => {
                uncovered = Rectangle::subtract_rects_many_in_place(uncovered, [*rect]);
                if uncovered.is_empty() {
                    return true;
                }
                if uncovered.len() > MAX_RECTS {
                    return false;
                }
            }
            RectangleKind::Subtract if uncovered.iter().any(|area| area.overlaps(*rect)) => {
                return false;
            }
            RectangleKind::Subtract => {}
        }
    }
    false
}

#[cfg(test)]
#[path = "client_shape_tests.rs"]
mod tests;
