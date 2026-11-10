//! Prefer an explicit, live transient parent over unrelated hover history.
use super::{InnerServerState, SurfaceRole, WindowData, X11Selection};
use hecs::Entity;
use xcb::x;

impl<S: X11Selection> InnerServerState<S> {
    pub(super) fn popup_parent(&self, popup: Entity) -> Option<x::Window> {
        let transient = self
            .world
            .get::<&WindowData>(popup)
            .ok()?
            .attrs
            .transient_for;
        let live = |window: &x::Window| {
            self.windows.get(window).is_some_and(|entity| {
                *entity != popup
                    && self
                        .world
                        .get::<&WindowData>(*entity)
                        .is_ok_and(|data| data.mapped)
                    && self
                        .world
                        .get::<&SurfaceRole>(*entity)
                        .is_ok_and(|role| role.xdg().is_some())
            })
        };
        transient
            .filter(live)
            .or_else(|| self.last_hovered.filter(live))
            .or_else(|| self.last_focused_toplevel.filter(live))
    }
}
