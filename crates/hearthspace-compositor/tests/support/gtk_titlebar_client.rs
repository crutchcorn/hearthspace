//! GTK protocol extension of the existing real SHM client.
use super::{InteractiveClient, State as ClientState};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, protocol::wl_registry};

mod protocol {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports, clippy::all)]
    use wayland_client;
    use wayland_client::protocol::*;
    mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/gtk-shell.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/gtk-shell.xml");
}

pub(crate) use protocol::gtk_surface1::Gesture;
use protocol::{gtk_shell1, gtk_surface1};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConfigureEvent {
    Gtk,
    Edges(Vec<u32>),
    Toplevel,
    Surface,
}

#[derive(Default)]
pub(super) struct State {
    shell: Option<gtk_shell1::GtkShell1>,
    capabilities: Option<u32>,
    surfaces: Vec<(usize, gtk_surface1::GtkSurface1)>,
    edges: std::collections::HashMap<usize, Vec<u32>>,
    registry: Option<(wl_registry::WlRegistry, u32)>,
    events: Vec<(usize, ConfigureEvent)>,
}

impl State {
    pub(super) fn bind(
        &mut self,
        registry: &wl_registry::WlRegistry,
        name: u32,
        version: u32,
        queue: &QueueHandle<ClientState>,
    ) {
        assert_eq!(version, 5, "GTK v6 promises unsupported surface offsets");
        self.registry = Some((registry.clone(), name));
        self.shell = Some(registry.bind(name, version, queue, ()));
    }

    pub(super) fn record_configure(&mut self, number: usize, surface: bool) {
        if self.surfaces.iter().any(|(index, _)| *index == number) {
            self.events.push((
                number,
                if surface {
                    ConfigureEvent::Surface
                } else {
                    ConfigureEvent::Toplevel
                },
            ));
        }
    }
}

impl InteractiveClient {
    pub(crate) fn gtk_use_version(&mut self, number: usize, version: u32) {
        assert!(
            !self
                .state
                .gtk
                .surfaces
                .iter()
                .any(|(index, _)| *index == number)
        );
        let (registry, name) = self.state.gtk.registry.as_ref().unwrap();
        let shell: gtk_shell1::GtkShell1 = registry.bind(*name, version, &self.queue.handle(), ());
        let surface = shell.get_gtk_surface(
            &self.state.windows[number].surface,
            &self.queue.handle(),
            number,
        );
        assert_eq!(surface.version(), version);
        self.state.gtk.surfaces.push((number, surface));
        self.sync();
    }

    pub(crate) fn gtk_configure_events(&mut self, number: usize) -> Vec<ConfigureEvent> {
        self.sync();
        std::mem::take(&mut self.state.gtk.events)
            .into_iter()
            .filter_map(|(index, event)| (index == number).then_some(event))
            .collect()
    }

    fn gtk_surface(&mut self, number: usize) -> gtk_surface1::GtkSurface1 {
        if let Some((_, surface)) = self
            .state
            .gtk
            .surfaces
            .iter()
            .find(|(index, _)| *index == number)
        {
            return surface.clone();
        }
        let surface = self
            .state
            .gtk
            .shell
            .as_ref()
            .expect("GTK shell advertised")
            .get_gtk_surface(
                &self.state.windows[number].surface,
                &self.queue.handle(),
                number,
            );
        self.state.gtk.surfaces.push((number, surface.clone()));
        surface
    }

    pub(crate) fn gtk_gesture(&mut self, number: usize, serial: u32, gesture: Gesture) {
        self.gtk_surface(number).titlebar_gesture(
            serial,
            self.state.seat.as_ref().unwrap(),
            gesture,
        );
        self.sync();
    }

    pub(crate) fn gtk_recreate_surface(&mut self, number: usize) {
        self.state.gtk.surfaces.retain(|(index, surface)| {
            if *index == number {
                surface.release();
                false
            } else {
                true
            }
        });
        self.gtk_surface(number);
        self.sync();
    }

    pub(crate) fn gtk_baseline_requests(&mut self, number: usize) {
        let gtk = self.gtk_surface(number);
        let shell = self.state.gtk.shell.as_ref().unwrap().clone();
        shell.set_startup_id(Some("untrusted-startup".into()));
        shell.notify_launch("untrusted-startup".into());
        shell.system_bell(Some(&gtk));
        gtk.set_dbus_properties(
            Some("dev.hearthspace.Untrusted".into()),
            None,
            None,
            None,
            None,
            None,
        );
        gtk.set_modal();
        gtk.unset_modal();
        gtk.present(0);
        gtk.request_focus(Some("untrusted-startup".into()));
        self.sync();
        assert_eq!(self.state.gtk.capabilities, Some(0));
        assert_eq!(shell.version(), 5);
    }

    pub(crate) fn gtk_edges(&mut self, number: usize) -> Vec<u32> {
        self.gtk_surface(number);
        self.sync();
        self.state
            .gtk
            .edges
            .get(&number)
            .cloned()
            .expect("GTK resize edges received")
    }

    pub(crate) fn gtk_request_maximized(&mut self, number: usize, enabled: bool) {
        if enabled {
            self.state.windows[number].toplevel.set_maximized();
        } else {
            self.state.windows[number].toplevel.unset_maximized();
        }
        self.sync();
    }

    pub(crate) fn gtk_invalid_gesture_disconnects(&mut self, number: usize, serial: u32) {
        self.gtk_surface(number)
            .send_request(gtk_surface1::Request::TitlebarGesture {
                serial,
                seat: self.state.seat.as_ref().unwrap().clone(),
                gesture: wayland_client::WEnum::Unknown(99),
            })
            .unwrap();
        assert!(self.queue.roundtrip(&mut self.state).is_err());
        let error = self
            ._connection
            .protocol_error()
            .expect("GTK protocol error");
        assert_eq!(error.object_interface, "gtk_surface1");
        assert_eq!(error.code, gtk_surface1::Error::InvalidGesture as u32);
    }
}

impl Dispatch<gtk_shell1::GtkShell1, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &gtk_shell1::GtkShell1,
        event: gtk_shell1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let gtk_shell1::Event::Capabilities { capabilities } = event;
        state.gtk.capabilities = Some(capabilities);
    }
}

impl Dispatch<gtk_surface1::GtkSurface1, usize> for ClientState {
    fn event(
        state: &mut Self,
        _: &gtk_surface1::GtkSurface1,
        event: gtk_surface1::Event,
        number: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            gtk_surface1::Event::Configure { .. } => {
                state.gtk.events.push((*number, ConfigureEvent::Gtk))
            }
            gtk_surface1::Event::ConfigureEdges { constraints } => {
                let edges: Vec<_> = constraints
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| u32::from_ne_bytes(*bytes))
                    .collect();
                state
                    .gtk
                    .events
                    .push((*number, ConfigureEvent::Edges(edges.clone())));
                state.gtk.edges.insert(*number, edges);
            }
        }
    }
}
