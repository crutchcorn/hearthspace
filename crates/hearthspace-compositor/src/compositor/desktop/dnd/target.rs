//! Smithay owns host Wayland offers; Satellite translates its X11 destinations.
use super::{App, source::Files};
use smithay::{
    backend::input::InputTime,
    input::dnd::{DndFocus, OfferData},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{IsAlive, Logical, Point, SERIAL_COUNTER},
    wayland::selection::data_device::WlOfferData,
};
use std::sync::Arc;

#[derive(Clone, PartialEq)]
pub(super) struct Target(pub(super) WlSurface);
pub(super) struct Offer(WlOfferData<Files>);

impl Offer {
    pub(super) fn validated(&self) -> bool {
        self.0.validated()
    }
    pub(super) fn disable(&self) {
        self.0.disable();
    }
    pub(super) fn dropped(&self) {
        OfferData::drop(&self.0);
    }
}

impl Target {
    pub(super) fn alive(&self) -> bool {
        self.0.alive()
    }
    pub(super) fn enter(
        &self,
        app: &mut App,
        source: Arc<Files>,
        point: Point<f64, Logical>,
    ) -> Option<Offer> {
        let dh = app.display_handle.clone();
        let seat = app._seat.clone();
        let serial = SERIAL_COUNTER.next_serial();
        DndFocus::enter(&self.0, app, &dh, source, &seat, point, &serial).map(Offer)
    }
    pub(super) fn motion(
        &self,
        app: &mut App,
        offer: Option<&mut Offer>,
        point: Point<f64, Logical>,
        time: InputTime,
    ) {
        let seat = app._seat.clone();
        DndFocus::motion(
            &self.0,
            app,
            offer.map(|offer| &mut offer.0),
            &seat,
            point,
            time,
        );
    }
    pub(super) fn leave(&self, app: &mut App, offer: Option<&mut Offer>) {
        if self.alive() {
            let seat = app._seat.clone();
            DndFocus::leave(&self.0, app, offer.map(|offer| &mut offer.0), &seat);
        }
    }
    pub(super) fn drop(&self, app: &mut App, offer: Option<&mut Offer>) {
        if self.alive() {
            let seat = app._seat.clone();
            DndFocus::drop(&self.0, app, offer.map(|offer| &mut offer.0), &seat);
        }
    }
}
