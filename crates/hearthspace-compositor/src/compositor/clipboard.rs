//! Seat clipboard integration. Smithay owns offers and asynchronous data transfer.
use std::{os::fd::OwnedFd, sync::Arc};

use smithay::{
    input::Seat,
    wayland::selection::{
        SelectionHandler, SelectionTarget,
        data_device::{DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler},
        primary_selection::{PrimarySelectionHandler, PrimarySelectionState},
    },
};

mod files;
mod text;
pub(in crate::compositor) use text::send_bytes;

#[cfg(feature = "xwayland")]
use {
    smithay::{
        wayland::selection::{
            SelectionSource, data_device::current_data_device_selection_userdata,
            primary_selection::current_primary_selection_userdata,
        },
        xwayland::xwm::XwmId,
    },
    tracing::warn,
};

use super::App;

#[derive(Clone)]
pub(super) enum SelectionData {
    Text(Arc<[u8]>),
    Files(Arc<hearthspace_common::desktop_files::FileClipboard>),
    #[cfg(feature = "xwayland")]
    X11(ClipboardSource),
}

/// Identifies the XWM and the particular selection advertised by it. Wayland
/// offers can outlive ownership changes; an old offer must never read a later
/// owner's data through XWayland's current-selection API.
#[cfg(feature = "xwayland")]
#[derive(Clone)]
pub(super) struct ClipboardSource {
    pub(super) xwm: XwmId,
    generation: Arc<()>,
    mime_types: Arc<[String]>,
}

#[cfg(feature = "xwayland")]
impl ClipboardSource {
    pub(super) fn new(xwm: XwmId, mime_types: &[String]) -> Self {
        Self {
            xwm,
            generation: Arc::new(()),
            mime_types: mime_types.into(),
        }
    }

    fn is_same_selection(&self, other: &Self) -> bool {
        self.xwm == other.xwm && Arc::ptr_eq(&self.generation, &other.generation)
    }
}

impl SelectionHandler for App {
    type SelectionUserData = SelectionData;

    #[cfg(feature = "xwayland")]
    fn new_selection(
        &mut self,
        target: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        if self.xdisplay.is_some()
            && let Some(xwm) = self.xwm.as_mut()
            && let Err(error) = xwm.new_selection(target, source.map(|source| source.mime_types()))
        {
            warn!(%error, ?target, "failed to advertise Wayland selection to XWayland");
        }
    }

    fn send_selection(
        &mut self,
        target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        seat: Seat<Self>,
        source: &SelectionData,
    ) {
        match source {
            SelectionData::Text(bytes) => text::send_text(bytes.clone(), &mime_type, fd),
            SelectionData::Files(files) => {
                if let Some(bytes) = files.encode(&mime_type) {
                    send_bytes(bytes.into(), fd);
                }
            }
            #[cfg(feature = "xwayland")]
            SelectionData::X11(source) => {
                self.send_x11_selection(target, mime_type, fd, seat, source)
            }
        }
        #[cfg(not(feature = "xwayland"))]
        let _ = (target, seat);
    }
}

#[cfg(feature = "xwayland")]
impl App {
    fn send_x11_selection(
        &mut self,
        target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        seat: Seat<Self>,
        source: &ClipboardSource,
    ) {
        let current = match target {
            SelectionTarget::Clipboard => current_data_device_selection_userdata(&seat),
            SelectionTarget::Primary => current_primary_selection_userdata(&seat),
        };
        if !current.is_some_and(|current| matches!(&*current, SelectionData::X11(current) if current.is_same_selection(source))) {
            return;
        }
        if let Some(xwm) = self.xwm.as_mut().filter(|xwm| xwm.id() == source.xwm)
            && let Err(error) = xwm.send_selection(target, mime_type, fd)
        {
            warn!(%error, ?target, "failed to transfer X11 selection to Wayland");
        }
    }
}

impl DataDeviceHandler for App {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl WaylandDndGrabHandler for App {}

impl PrimarySelectionHandler for App {
    fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
        &mut self.primary_selection_state
    }
}
