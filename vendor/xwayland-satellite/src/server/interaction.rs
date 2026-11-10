//! Optional, interaction-scoped cancellation. No authority to start operations.
use super::*;
use protocol::xwayland_satellite_interaction_v1::XwaylandSatelliteInteractionV1;

pub(super) mod protocol {
    #![allow(non_camel_case_types, non_upper_case_globals, unused_imports)]
    use wayland_client::{self, protocol::*};
    use wayland_protocols::xdg::shell::client::*;
    mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols::xdg::shell::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/xwayland-satellite-interaction-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/xwayland-satellite-interaction-v1.xml");
}

wayland_client::delegate_noop!(MyWorld: XwaylandSatelliteInteractionV1);

pub(super) struct StartedInteraction {
    seat: client::wl_seat::WlSeat,
    serial: u32,
    toplevel: XdgToplevel,
}

impl<S: X11Selection> InnerServerState<S> {
    pub(super) fn request_window_interaction(
        &mut self,
        window: x::Window,
        edge: Option<xdg_toplevel::ResizeEdge>,
    ) {
        let Some(&entity) = self.windows.get(&window) else {
            return;
        };
        let started = {
            let Ok(data) = self.world.entity(entity) else {
                return;
            };
            let Some(click) = data.get::<&LastClickSerial>() else {
                warn!("Requested interaction without a click serial: {window:?}");
                return;
            };
            let role = data.get::<&SurfaceRole>();
            let Some(SurfaceRole::Toplevel(Some(toplevel))) = role.as_deref() else {
                return;
            };
            StartedInteraction {
                seat: click.0.clone(),
                serial: click.1,
                toplevel: toplevel.toplevel.clone(),
            }
        };
        match edge {
            Some(edge) => started.toplevel.resize(&started.seat, started.serial, edge),
            None => started.toplevel._move(&started.seat, started.serial),
        }
        self.world.insert_one(entity, started).unwrap();
    }

    pub fn cancel_window_interaction(&mut self, window: x::Window) {
        let Some(&entity) = self.windows.get(&window) else {
            return;
        };
        let Ok(started) = self.world.remove_one::<StartedInteraction>(entity) else {
            return;
        };
        if !started.toplevel.is_alive() || !started.seat.is_alive() {
            return;
        }
        if let Some(manager) = &self.interaction_manager {
            manager.cancel(&started.toplevel, &started.seat, started.serial);
        } else {
            warn!("Compositor does not support cancelling window interactions");
        }
    }
}
