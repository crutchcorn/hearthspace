//! The host supplies appearance; Satellite remains the XSettings manager.
use super::clientside::MyWorld;
use wayland_client::{Connection, Dispatch, QueueHandle};
pub(super) mod protocol {
    #![allow(non_camel_case_types, non_upper_case_globals, unused_imports)]
    use wayland_client::{self, protocol::*};
    mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/xwayland-satellite-settings-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/xwayland-satellite-settings-v1.xml");
}
use protocol::xwayland_satellite_settings_v1::{self, XwaylandSatelliteSettingsV1};
impl Dispatch<XwaylandSatelliteSettingsV1, ()> for MyWorld {
    fn event(
        state: &mut Self,
        _: &XwaylandSatelliteSettingsV1,
        event: xwayland_satellite_settings_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let xwayland_satellite_settings_v1::Event::ThemeName { name } = event;
        state.theme_name = Some(name);
    }
}
impl<S: crate::X11Selection> super::InnerServerState<S> {
    pub(crate) fn new_theme_name(&mut self) -> Option<String> {
        self.world.theme_name.take()
    }
}
