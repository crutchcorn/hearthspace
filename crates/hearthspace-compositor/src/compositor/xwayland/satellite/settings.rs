//! Appearance delivery only to the private Satellite client.
use crate::compositor::App;
pub(super) use protocol::xwayland_satellite_settings_v1::XwaylandSatelliteSettingsV1;
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, backend::ClientId,
};
mod protocol {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports, clippy::all)]
    use wayland_server::{self, protocol::*};
    mod __interfaces {
        use wayland_server::backend as wayland_backend;
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "../../vendor/xwayland-satellite/protocols/xwayland-satellite-settings-v1.xml"
        );
    }
    use self::__interfaces::*;
    wayland_scanner::generate_server_code!(
        "../../vendor/xwayland-satellite/protocols/xwayland-satellite-settings-v1.xml"
    );
}
impl App {
    fn satellite_theme_name(&self) -> &'static str {
        match self.appearance.color_mode {
            hearthspace_ipc::ColorMode::Light => "Adwaita",
            hearthspace_ipc::ColorMode::Dark => "Adwaita-dark",
        }
    }
    pub(in crate::compositor) fn publish_xsettings(&mut self) {
        let theme = self.satellite_theme_name();
        self.satellite_interaction.settings.retain(|resource| {
            if let Ok(resource) = resource.upgrade() {
                resource.theme_name(theme.to_owned());
                true
            } else {
                false
            }
        });
    }
}
impl GlobalDispatch<XwaylandSatelliteSettingsV1, ClientId> for App {
    fn bind(
        state: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<XwaylandSatelliteSettingsV1>,
        _: &ClientId,
        init: &mut DataInit<'_, Self>,
    ) {
        let resource = init.init(resource, ());
        resource.theme_name(state.satellite_theme_name().to_owned());
        state
            .satellite_interaction
            .settings
            .push(resource.downgrade());
    }
    fn can_view(client: Client, authorized: &ClientId) -> bool {
        client.id() == *authorized
    }
}
impl Dispatch<XwaylandSatelliteSettingsV1, ()> for App {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &XwaylandSatelliteSettingsV1,
        _: protocol::xwayland_satellite_settings_v1::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}
