//! Host URI-list drag events, kept in Wayland dispatch order.
use super::{InnerServerState, SurfaceScaleFactor, WindowData, X11Selection};
use hecs::Entity;
use smithay_client_toolkit::data_device_manager::data_offer::DragOffer;
use wayland_client::Proxy;
use xcb::x;

pub(crate) enum Event {
    Enter(DragOffer),
    Motion(DragOffer),
    Drop(DragOffer),
    Leave,
}

impl<S: X11Selection> InnerServerState<S> {
    pub(crate) fn take_drag_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.world.drag_events)
    }

    pub(crate) fn drag_target(&self, offer: &DragOffer) -> Option<(x::Window, f64, (i16, i16))> {
        let entity = *offer.surface.data::<Entity>()?;
        let mut query = self
            .world
            .query_one::<(&x::Window, &WindowData, &SurfaceScaleFactor)>(entity)
            .ok()?;
        let (window, data, scale) = query.get()?;
        data.mapped
            .then_some((*window, scale.0, (data.attrs.dims.x, data.attrs.dims.y)))
    }
}
