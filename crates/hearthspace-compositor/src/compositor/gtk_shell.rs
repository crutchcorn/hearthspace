//! GTK's explicit titlebar gestures, independent of generic maximize requests.
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
    protocol::wl_surface::WlSurface,
};

use super::App;
use protocol::{gtk_shell1, gtk_surface1};

mod configure;
mod gesture;
mod protocol;
pub(super) use configure::{send_configure, send_pending_configure};

#[derive(Default)]
pub(super) struct GtkShellState {
    press: Option<gesture::TitlebarPress>,
}

impl GtkShellState {
    pub(super) fn new(display: &DisplayHandle) -> Self {
        // Version 6 also promises surface-offset support, which we do not assert.
        display.create_global::<App, gtk_shell1::GtkShell1, _>(5, ());
        Self::default()
    }

    pub(super) fn reset_input(&mut self) {
        self.press = None;
    }
}

pub(super) struct GtkSurfaceData {
    surface: WlSurface,
}

impl GlobalDispatch<gtk_shell1::GtkShell1, ()> for App {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<gtk_shell1::GtkShell1>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        // App menus, menu bars and desktop icons remain client-owned.
        init.init(resource, ()).capabilities(0);
    }
}

impl Dispatch<gtk_shell1::GtkShell1, ()> for App {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &gtk_shell1::GtkShell1,
        request: gtk_shell1::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            gtk_shell1::Request::GetGtkSurface {
                gtk_surface,
                surface,
            } => {
                let gtk = init.init(
                    gtk_surface,
                    GtkSurfaceData {
                        surface: surface.clone(),
                    },
                );
                configure::track(&surface, &gtk);
                if let Some(toplevel) = state
                    .windows
                    .iter()
                    .filter_map(|window| window.surface.xdg_toplevel())
                    .find(|toplevel| toplevel.wl_surface() == &surface)
                    && toplevel.is_initial_configure_sent()
                {
                    send_configure(toplevel);
                }
            }
            // Startup notification and bells are advisory. They do not grant
            // activation authority or create additional compositor surfaces.
            gtk_shell1::Request::SetStartupId { .. }
            | gtk_shell1::Request::NotifyLaunch { .. }
            | gtk_shell1::Request::SystemBell { .. } => {}
        }
    }
}

impl Dispatch<gtk_surface1::GtkSurface1, GtkSurfaceData> for App {
    fn request(
        state: &mut Self,
        _: &Client,
        resource: &gtk_surface1::GtkSurface1,
        request: gtk_surface1::Request,
        data: &GtkSurfaceData,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        match request {
            gtk_surface1::Request::TitlebarGesture {
                serial,
                seat,
                gesture,
            } => match gesture {
                WEnum::Value(gesture) => {
                    state.gtk_titlebar_gesture(&data.surface, &seat, serial.into(), gesture);
                }
                WEnum::Unknown(value) => resource.post_error(
                    gtk_surface1::Error::InvalidGesture,
                    format!("unknown GTK titlebar gesture {value}"),
                ),
            },
            // These are hints, not authenticated activation requests. Existing
            // xdg parent/activation rules continue to own focus and transients.
            // Gtk configure/edges are emitted with each xdg configure by the
            // shared configure wrappers; these requests do not change them.
            gtk_surface1::Request::SetDbusProperties { .. }
            | gtk_surface1::Request::SetModal
            | gtk_surface1::Request::UnsetModal
            | gtk_surface1::Request::Present { .. }
            | gtk_surface1::Request::RequestFocus { .. }
            | gtk_surface1::Request::Release => {}
            _ => {}
        }
    }
}
