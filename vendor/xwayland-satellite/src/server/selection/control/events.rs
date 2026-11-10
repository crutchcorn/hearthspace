//! Only source events are consumed here; host offers are deliberately unread.
use super::MyWorld;
use std::os::fd::OwnedFd;
use wayland_client::{Connection, Dispatch, QueueHandle, event_created_child};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1 as Device},
    ext_data_control_manager_v1::ExtDataControlManagerV1 as Manager,
    ext_data_control_offer_v1::ExtDataControlOfferV1 as Offer,
    ext_data_control_source_v1::{self, ExtDataControlSourceV1 as Source},
};
#[derive(Default)]
pub(crate) struct Events {
    pub requests: Vec<(Source, String, OwnedFd)>,
    pub cancelled: Vec<Source>,
}
wayland_client::delegate_noop!(MyWorld: Manager);
wayland_client::delegate_noop!(MyWorld: ignore Offer);
impl Dispatch<Device, ()> for MyWorld {
    fn event(
        _: &mut Self,
        device: &Device,
        event: ext_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::Selection { id }
            | ext_data_control_device_v1::Event::PrimarySelection { id } => {
                if let Some(offer) = id {
                    offer.destroy();
                }
            }
            ext_data_control_device_v1::Event::Finished => device.destroy(),
            _ => {}
        }
    }
    event_created_child!(MyWorld, Device, [ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (Offer, ())]);
}
impl Dispatch<Source, ()> for MyWorld {
    fn event(
        state: &mut Self,
        source: &Source,
        event: ext_data_control_source_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { mime_type, fd } => state
                .control_events
                .requests
                .push((source.clone(), mime_type, fd)),
            ext_data_control_source_v1::Event::Cancelled => {
                state.control_events.cancelled.push(source.clone())
            }
            _ => {}
        }
    }
}
