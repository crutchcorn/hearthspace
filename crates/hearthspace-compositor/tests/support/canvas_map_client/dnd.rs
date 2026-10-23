//! Real data-device receiver; offers are accepted only when enabled by a test.
use super::{MapClient, State as ClientState};
use std::{
    io::{Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    time::Duration,
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop, event_created_child,
    protocol::{
        wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_registry, wl_seat,
    },
};

#[derive(Default)]
pub(super) struct State {
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    pub(super) seat: Option<wl_seat::WlSeat>,
    device: Option<wl_data_device::WlDataDevice>,
    offer: Option<wl_data_offer::WlDataOffer>,
    accept: bool,
    pub(super) enters: usize,
    drops: usize,
    readers: Vec<UnixStream>,
    selection: Option<wl_data_offer::WlDataOffer>,
    source: Option<wl_data_source::WlDataSource>,
    clipboard: Vec<u8>,
}

pub(super) fn global(
    state: &mut ClientState,
    registry: &wl_registry::WlRegistry,
    name: u32,
    interface: &str,
    version: u32,
    handle: &QueueHandle<ClientState>,
) {
    if interface == "wl_data_device_manager" {
        state.dnd.manager = Some(registry.bind(name, version.min(3), handle, ()));
    }
}

impl MapClient {
    pub(in super::super) fn enable_file_drops(&mut self, accept: bool) {
        self.state.dnd.accept = accept;
        self.state.dnd.device = Some(self.state.dnd.manager.as_ref().unwrap().get_data_device(
            self.state.dnd.seat.as_ref().unwrap(),
            &self.queue.handle(),
            (),
        ));
        self.sync();
    }
    pub(in super::super) fn drop_enters(&mut self) -> usize {
        self.sync();
        self.state.dnd.enters
    }
    pub(in super::super) fn drops(&mut self) -> usize {
        self.sync();
        self.state.dnd.drops
    }
    pub(in super::super) fn dropped_files(&mut self) -> Vec<Vec<u8>> {
        self.sync();
        self.state
            .dnd
            .readers
            .drain(..)
            .map(|mut reader| {
                reader
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes).unwrap();
                bytes
            })
            .collect()
    }
    pub(in super::super) fn own_file_clipboard(&mut self, bytes: Vec<u8>) {
        let source = self
            .state
            .dnd
            .manager
            .as_ref()
            .unwrap()
            .create_data_source(&self.queue.handle(), ());
        source.offer("x-special/gnome-copied-files".into());
        self.state.dnd.clipboard = bytes;
        self.state
            .dnd
            .device
            .as_ref()
            .unwrap()
            .set_selection(Some(&source), 0);
        self.state.dnd.source = Some(source);
        self.sync();
    }
    pub(in super::super) fn clipboard_files(&mut self, mime: &str) -> Vec<u8> {
        self.sync();
        let (mut reader, writer) = UnixStream::pair().unwrap();
        self.state
            .dnd
            .selection
            .as_ref()
            .unwrap()
            .receive(mime.into(), writer.as_fd());
        drop(writer);
        self.sync();
        reader
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Enter { serial, id, .. } => {
                state.dnd.enters += 1;
                if let Some(offer) = &id {
                    offer.accept(serial, state.dnd.accept.then(|| "text/uri-list".into()));
                    offer.set_actions(
                        wl_data_device_manager::DndAction::Copy,
                        wl_data_device_manager::DndAction::Copy,
                    );
                }
                state.dnd.offer = id;
            }
            wl_data_device::Event::Drop => {
                state.dnd.drops += 1;
                if state.dnd.accept
                    && let Some(offer) = &state.dnd.offer
                {
                    let (reader, writer) = UnixStream::pair().unwrap();
                    offer.receive("text/uri-list".into(), writer.as_fd());
                    state.dnd.readers.push(reader);
                    offer.finish();
                }
            }
            wl_data_device::Event::Leave => {
                state.dnd.offer = None;
            }
            wl_data_device::Event::Selection { id } => {
                state.dnd.selection = id;
            }
            _ => {}
        }
    }
    event_created_child!(ClientState, wl_data_device::WlDataDevice, [0 => (wl_data_offer::WlDataOffer, ())]);
}
delegate_noop!(ClientState: ignore wl_data_device_manager::WlDataDeviceManager);
delegate_noop!(ClientState: ignore wl_data_offer::WlDataOffer);

impl Dispatch<wl_data_source::WlDataSource, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_source::Event::Send { fd, .. } = event {
            std::fs::File::from(fd)
                .write_all(&state.dnd.clipboard)
                .unwrap();
        }
    }
}
