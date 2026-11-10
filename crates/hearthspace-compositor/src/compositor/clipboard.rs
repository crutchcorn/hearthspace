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

use super::App;

#[derive(Clone)]
pub(super) enum SelectionData {
    Text(Arc<[u8]>),
    Files(Arc<hearthspace_common::desktop_files::FileClipboard>),
}

impl SelectionHandler for App {
    type SelectionUserData = SelectionData;

    fn send_selection(
        &mut self,
        _target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        source: &SelectionData,
    ) {
        match source {
            SelectionData::Text(bytes) => text::send_text(bytes.clone(), &mime_type, fd),
            SelectionData::Files(files) => {
                if let Some(bytes) = files.encode(&mime_type) {
                    send_bytes(bytes.into(), fd);
                }
            }
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
