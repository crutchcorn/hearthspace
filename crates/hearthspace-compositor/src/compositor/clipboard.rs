//! Seat clipboard integration. Smithay owns offers and asynchronous data transfer.
use smithay::wayland::selection::{
    SelectionHandler,
    data_device::{DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler},
};

#[cfg(feature = "xwayland")]
use {
    smithay::{
        input::Seat,
        wayland::selection::{
            SelectionSource, SelectionTarget, data_device::current_data_device_selection_userdata,
        },
        xwayland::xwm::XwmId,
    },
    std::{os::fd::OwnedFd, sync::Arc},
    tracing::warn,
};

use super::App;

/// Identifies the XWM and the particular selection advertised by it. Wayland
/// offers can outlive ownership changes; an old offer must never read a later
/// owner's data through XWayland's current-selection API.
#[cfg(feature = "xwayland")]
#[derive(Clone)]
pub(super) struct ClipboardSource {
    pub(super) xwm: XwmId,
    generation: Arc<()>,
}

#[cfg(feature = "xwayland")]
impl ClipboardSource {
    pub(super) fn new(xwm: XwmId) -> Self {
        Self {
            xwm,
            generation: Arc::new(()),
        }
    }

    fn is_same_selection(&self, other: &Self) -> bool {
        self.xwm == other.xwm && Arc::ptr_eq(&self.generation, &other.generation)
    }
}

impl SelectionHandler for App {
    #[cfg(feature = "xwayland")]
    type SelectionUserData = ClipboardSource;
    #[cfg(not(feature = "xwayland"))]
    type SelectionUserData = ();

    #[cfg(feature = "xwayland")]
    fn new_selection(
        &mut self,
        target: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        if target == SelectionTarget::Clipboard
            && self.xdisplay.is_some()
            && let Some(xwm) = self.xwm.as_mut()
            && let Err(error) = xwm.new_selection(target, source.map(|source| source.mime_types()))
        {
            warn!(%error, "failed to advertise Wayland clipboard to XWayland");
        }
    }

    #[cfg(feature = "xwayland")]
    fn send_selection(
        &mut self,
        target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        seat: Seat<Self>,
        source: &ClipboardSource,
    ) {
        if target != SelectionTarget::Clipboard
            || !current_data_device_selection_userdata(&seat)
                .is_some_and(|current| current.is_same_selection(source))
        {
            return;
        }
        if let Some(xwm) = self.xwm.as_mut().filter(|xwm| xwm.id() == source.xwm)
            && let Err(error) = xwm.send_selection(target, mime_type, fd)
        {
            warn!(%error, "failed to transfer X11 clipboard to Wayland");
        }
    }
}

impl DataDeviceHandler for App {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl WaylandDndGrabHandler for App {}
