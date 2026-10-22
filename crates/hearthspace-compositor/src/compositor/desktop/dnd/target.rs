//! Smithay owns Wayland data offers and XDND negotiation, including transfer FDs.
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
pub(super) enum Target {
    Wayland(WlSurface),
    #[cfg(feature = "xwayland")]
    X11(Box<smithay::xwayland::X11Surface>),
}

pub(super) enum Offer {
    Wayland(WlOfferData<Files>),
    #[cfg(feature = "xwayland")]
    X11(smithay::xwayland::xwm::XwmOfferData<Files>),
}

impl Offer {
    pub(super) fn validated(&self) -> bool {
        match self {
            Self::Wayland(offer) => offer.validated(),
            #[cfg(feature = "xwayland")]
            Self::X11(offer) => offer.validated(),
        }
    }
    pub(super) fn disable(&self) {
        match self {
            Self::Wayland(offer) => offer.disable(),
            #[cfg(feature = "xwayland")]
            Self::X11(offer) => offer.disable(),
        }
    }
    pub(super) fn dropped(&self) {
        match self {
            Self::Wayland(offer) => OfferData::drop(offer),
            #[cfg(feature = "xwayland")]
            Self::X11(offer) => OfferData::drop(offer),
        }
    }
}

impl Target {
    pub(super) fn alive(&self, _app: &App) -> bool {
        match self {
            Self::Wayland(surface) => surface.alive(),
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => {
                surface.alive()
                    && _app
                        .xwm
                        .as_ref()
                        .is_some_and(|xwm| Some(xwm.id()) == surface.xwm_id())
            }
        }
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
        match self {
            Self::Wayland(surface) => {
                DndFocus::enter(surface, app, &dh, source, &seat, point, &serial)
                    .map(Offer::Wayland)
            }
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => {
                DndFocus::enter(surface.as_ref(), app, &dh, source, &seat, point, &serial)
                    .map(Offer::X11)
            }
        }
    }
    pub(super) fn motion(
        &self,
        app: &mut App,
        offer: Option<&mut Offer>,
        point: Point<f64, Logical>,
        time: InputTime,
    ) {
        let seat = app._seat.clone();
        match (self, offer) {
            (Self::Wayland(surface), Some(Offer::Wayland(offer))) => {
                DndFocus::motion(surface, app, Some(offer), &seat, point, time)
            }
            #[cfg(feature = "xwayland")]
            (Self::X11(surface), Some(Offer::X11(offer))) => {
                DndFocus::motion(surface.as_ref(), app, Some(offer), &seat, point, time)
            }
            _ => {}
        }
    }
    pub(super) fn leave(&self, app: &mut App, offer: Option<&mut Offer>) {
        if !self.alive(app) {
            return;
        }
        let seat = app._seat.clone();
        match (self, offer) {
            (Self::Wayland(surface), Some(Offer::Wayland(offer))) => {
                DndFocus::leave(surface, app, Some(offer), &seat)
            }
            #[cfg(feature = "xwayland")]
            (Self::X11(surface), Some(Offer::X11(offer))) => {
                DndFocus::leave(surface.as_ref(), app, Some(offer), &seat)
            }
            _ => {}
        }
    }
    pub(super) fn drop(&self, app: &mut App, offer: Option<&mut Offer>) {
        if !self.alive(app) {
            return;
        }
        let seat = app._seat.clone();
        match (self, offer) {
            (Self::Wayland(surface), Some(Offer::Wayland(offer))) => {
                DndFocus::drop(surface, app, Some(offer), &seat)
            }
            #[cfg(feature = "xwayland")]
            (Self::X11(surface), Some(Offer::X11(offer))) => {
                DndFocus::drop(surface.as_ref(), app, Some(offer), &seat)
            }
            _ => {}
        }
    }
}
