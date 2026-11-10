//! Only the private Satellite client can cancel its accepted XDG operation.
use smithay::{utils::Serial, wayland::shell::xdg::ToplevelSurface};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, Weak,
    backend::ClientId,
    protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
};

use crate::compositor::App;
use protocol::xwayland_satellite_interaction_v1::{Request, XwaylandSatelliteInteractionV1};

#[cfg(test)]
mod tests;

mod protocol {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports, clippy::all)]
    use wayland_protocols::xdg::shell::server::*;
    use wayland_server::{self, protocol::*};
    mod __interfaces {
        use wayland_protocols::xdg::shell::server::__interfaces::*;
        use wayland_server::backend as wayland_backend;
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "../../vendor/xwayland-satellite/protocols/xwayland-satellite-interaction-v1.xml"
        );
    }
    use self::__interfaces::*;
    wayland_scanner::generate_server_code!(
        "../../vendor/xwayland-satellite/protocols/xwayland-satellite-interaction-v1.xml"
    );
}

#[derive(Default)]
pub(in crate::compositor) struct State {
    client: Option<ClientId>,
    pub(super) data_control:
        Option<smithay::wayland::selection::ext_data_control::DataControlState>,
    active: Option<Interaction>,
    pub(super) settings: Vec<Weak<super::settings::XwaylandSatelliteSettingsV1>>,
}

struct Interaction {
    surface: Weak<WlSurface>,
    seat: WlSeat,
    serial: Serial,
    touch: bool,
}

impl State {
    pub(super) fn advertise(
        &mut self,
        dh: &DisplayHandle,
        client: ClientId,
        primary: &smithay::wayland::selection::primary_selection::PrimarySelectionState,
    ) {
        dh.create_global::<App, XwaylandSatelliteInteractionV1, _>(1, client.clone());
        dh.create_global::<App, super::settings::XwaylandSatelliteSettingsV1, _>(1, client.clone());
        let authorized = client.clone();
        self.data_control = Some(
            smithay::wayland::selection::ext_data_control::DataControlState::new::<App, _>(
                dh,
                Some(primary),
                move |client| client.id() == authorized,
            ),
        );
        self.client = Some(client);
    }

    pub(super) fn revoke(&mut self) {
        self.client = None;
        self.active = None;
        self.settings.clear();
    }
}

impl App {
    pub(in crate::compositor) fn track_satellite_interaction(
        &mut self,
        surface: &ToplevelSurface,
        seat: WlSeat,
        serial: Serial,
        touch: bool,
    ) {
        if surface.wl_surface().client().map(|client| client.id())
            != self.satellite_interaction.client
        {
            return;
        }
        self.satellite_interaction.active = Some(Interaction {
            surface: surface.wl_surface().downgrade(),
            seat,
            serial,
            touch,
        });
    }
}

impl GlobalDispatch<XwaylandSatelliteInteractionV1, ClientId> for App {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<XwaylandSatelliteInteractionV1>,
        _: &ClientId,
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }

    fn can_view(client: Client, authorized: &ClientId) -> bool {
        client.id() == *authorized
    }
}

impl Dispatch<XwaylandSatelliteInteractionV1, ()> for App {
    fn request(
        state: &mut Self,
        client: &Client,
        _: &XwaylandSatelliteInteractionV1,
        request: Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        let Request::Cancel {
            toplevel,
            seat,
            serial,
        } = request
        else {
            return;
        };
        if state.session_locked()
            || state.satellite_interaction.client.as_ref() != Some(&client.id())
        {
            return;
        }
        let Some(active) = state.satellite_interaction.active.as_ref() else {
            return;
        };
        if active.serial != serial.into() || active.seat != seat {
            return;
        }
        let Ok(surface) = active.surface.upgrade() else {
            return;
        };
        let Some(index) = state.windows.iter().position(|window| {
            window.mapped
                && window.surface.xdg_toplevel().is_some_and(|xdg| {
                    xdg.xdg_toplevel() == &toplevel && xdg.wl_surface() == &surface
                })
        }) else {
            return;
        };
        let id = state.windows[index].id;
        let owns_sequence = if active.touch {
            state.window_touch_operation_matches(id, active.serial)
        } else {
            state.window_pointer_operation_matches(id, active.serial)
        };
        if !owns_sequence {
            return;
        }
        state.satellite_interaction.active = None;
        // Finish before retirement so a late resized buffer retains its anchor.
        if state
            .resize
            .as_ref()
            .is_some_and(|resize| resize.window_id == id)
        {
            state.finish_resize();
        }
        state.retire_window_pointer_operation(id);
        state.request_redraw();
    }
}
