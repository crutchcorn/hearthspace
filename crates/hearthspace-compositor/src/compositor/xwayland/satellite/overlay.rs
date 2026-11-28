//! Authenticated, pre-map X11 overlay hints; no application-name rules.
use crate::compositor::{
    App, ManagedWindowKind,
    windows::overlays::{Hint, Hints},
};
pub(super) use protocol::xwayland_satellite_overlay_v1::XwaylandSatelliteOverlayV1;
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, backend::ClientId,
};
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
            "../../vendor/xwayland-satellite/protocols/xwayland-satellite-overlay-v1.xml"
        );
    }
    use self::__interfaces::*;
    wayland_scanner::generate_server_code!(
        "../../vendor/xwayland-satellite/protocols/xwayland-satellite-overlay-v1.xml"
    );
}

impl GlobalDispatch<XwaylandSatelliteOverlayV1, ClientId> for App {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<XwaylandSatelliteOverlayV1>,
        _: &ClientId,
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(resource, ());
    }
    fn can_view(client: Client, authorized: &ClientId) -> bool {
        client.id() == *authorized
    }
}
impl Dispatch<XwaylandSatelliteOverlayV1, ()> for App {
    fn request(
        state: &mut Self,
        client: &Client,
        _: &XwaylandSatelliteOverlayV1,
        request: protocol::xwayland_satellite_overlay_v1::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        let protocol::xwayland_satellite_overlay_v1::Request::SetOverlay {
            toplevel,
            output_name,
            corner,
        } = request
        else {
            return;
        };
        if corner > 3
            || output_name.len() > 1024
            || state.satellite_interaction.client.as_ref() != Some(&client.id())
        {
            return;
        }
        let Some(index) = state.windows.iter().position(|w| {
            w.surface
                .xdg_toplevel()
                .is_some_and(|s| s.xdg_toplevel() == &toplevel)
        }) else {
            return;
        };
        if state.windows[index].mapped {
            return;
        }
        let surface = state.windows[index].surface.wl_surface().unwrap();
        let hint = Hint {
            toplevel: toplevel.id(),
            corner,
            output: (!output_name.is_empty()).then_some(output_name),
        };
        smithay::wayland::compositor::with_states(&surface, |s| {
            if let Some(hints) = s.data_map.get::<Hints>() {
                *hints.0.lock().unwrap() = hint;
            } else {
                s.data_map
                    .insert_if_missing(|| Hints(std::sync::Mutex::new(hint)));
            }
        });
        let id = state.windows[index].id;
        state.reclassify_window(id, ManagedWindowKind::X11Overlay, None);
        if let Some(index) = state.windows.index(id) {
            let name = state.x11_overlay_output_name(index);
            state.enter_output(&surface, name.as_deref());
            if let Some(toplevel) = state.windows[index].surface.xdg_toplevel() {
                state.configure_toplevel(toplevel, ManagedWindowKind::X11Overlay, name.as_deref());
            }
        }
        tracing::debug!(id, corner, "classified X11 screen overlay");
        state.request_redraw();
    }
}

#[cfg(test)]
mod tests;
