//! XDG geometry is always intersected with the committed surface tree.
use smithay::{
    utils::{Logical, Point, Rectangle, Size},
    wayland::{compositor::with_states, shell::xdg::SurfaceCachedState},
};
use wayland_server::protocol::wl_surface::WlSurface;
fn geometry(surface: &WlSurface, bounds: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let requested = with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
    });
    requested
        .and_then(|rect| rect.intersection(bounds))
        .unwrap_or(bounds)
}
pub(in crate::compositor) fn toplevel_geometry_loc(
    surface: &WlSurface,
    bounds: Rectangle<i32, Logical>,
) -> Point<i32, Logical> {
    geometry(surface, bounds).loc
}
pub(in crate::compositor) fn toplevel_geometry_size(
    surface: &WlSurface,
    bounds: Rectangle<i32, Logical>,
) -> Option<Size<i32, Logical>> {
    Some(geometry(surface, bounds).size)
}
