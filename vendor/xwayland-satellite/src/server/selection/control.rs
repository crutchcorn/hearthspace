//! Export X11 selections without borrowing a focused application's serial.
//! Reading host selections still uses the ordinary focused data-device path.
use super::{MyWorld, SelectionType};
use crate::X11Selection;
use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};
use wayland_client::{Proxy, QueueHandle, globals::GlobalList};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::ExtDataControlDeviceV1 as Device,
    ext_data_control_manager_v1::ExtDataControlManagerV1 as Manager,
    ext_data_control_source_v1::ExtDataControlSourceV1 as Source,
};
mod events;
pub(crate) use events::Events;

pub(super) struct Control<S> {
    manager: Manager,
    device: Option<Device>,
    sources: HashMap<Source, (bool, Weak<S>)>,
    current: [Option<Source>; 2],
    pub(super) marker: String,
}
impl<S: X11Selection> Control<S> {
    pub(super) fn bind(globals: &GlobalList, qh: &QueueHandle<MyWorld>) -> Option<Self> {
        let manager = globals.bind(qh, 1..=1, ()).ok()?;
        let mut nonce = [0u8; 16];
        let mut filled = 0;
        while filled < nonce.len() {
            match rustix::rand::getrandom(
                &mut nonce[filled..],
                rustix::rand::GetRandomFlags::empty(),
            ) {
                Ok(n) if n > 0 => filled += n,
                Err(rustix::io::Errno::INTR) => continue,
                other => panic!("selection marker randomness failed: {other:?}"),
            }
        }
        let marker = format!(
            "application/x-satellite-selection-{}",
            nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
        );
        Some(Self {
            manager,
            device: None,
            sources: HashMap::new(),
            current: [None, None],
            marker,
        })
    }
    pub(super) fn seat_created(
        &mut self,
        qh: &QueueHandle<MyWorld>,
        seat: &wayland_client::protocol::wl_seat::WlSeat,
    ) {
        // Hearthspace has one seat. Never silently replace an existing device.
        if self.device.is_some() {
            return;
        }
        let device = self.manager.get_data_device(seat, qh, ());
        for (primary, source) in self.current.iter().enumerate() {
            if let Some(source) = source {
                if primary == 1 {
                    device.set_primary_selection(Some(source));
                } else {
                    device.set_selection(Some(source));
                }
            }
        }
        self.device = Some(device);
    }
    pub(super) fn set<T: SelectionType>(&mut self, qh: &QueueHandle<MyWorld>, selection: &Rc<S>) {
        let source = self.manager.create_data_source(qh, ());
        for mime in selection.mime_types() {
            source.offer(mime.to_owned());
        }
        // An echo marker prevents our own export becoming an X11 foreign owner.
        // It carries no data and is not an authorization token.
        source.offer(self.marker.clone());
        self.sources
            .insert(source.clone(), (T::PRIMARY, Rc::downgrade(selection)));
        if let Some(previous) = self.current[usize::from(T::PRIMARY)].replace(source.clone()) {
            self.sources.remove(&previous);
            previous.destroy();
        }
        if let Some(device) = &self.device {
            if T::PRIMARY {
                device.set_primary_selection(Some(&source));
            } else {
                device.set_selection(Some(&source));
            }
        }
    }
    pub(super) fn clear<T: SelectionType>(&mut self) {
        if let Some(source) = self.current[usize::from(T::PRIMARY)].take() {
            self.sources.remove(&source);
            // Source destruction only clears this generation, never a newer host copy.
            source.destroy();
        }
    }
    pub(super) fn dispatch(&mut self, events: &mut Events) -> Vec<(bool, Weak<S>)> {
        for (source, mime, fd) in std::mem::take(&mut events.requests) {
            if let Some((_, selection)) = self.sources.get(&source) {
                if mime != self.marker {
                    if let Some(selection) = selection.upgrade() {
                        selection.write_to(&mime, fd.into());
                    }
                }
            }
        }
        let mut retired = Vec::new();
        for source in std::mem::take(&mut events.cancelled) {
            if let Some((primary, selection)) = self.sources.remove(&source) {
                if self.current[usize::from(primary)].as_ref() == Some(&source) {
                    self.current[usize::from(primary)] = None;
                }
                retired.push((primary, selection));
                if source.is_alive() {
                    source.destroy();
                }
            }
        }
        retired
    }
}
