//! Real App, Wayland resources, XDG requests and Smithay pointer grabs.
//! Only mapping/buffer geometry and physical input are supplied by the fixture.
use crate::compositor::{
    App, CalloopData, client::ClientState as ServerClientState,
    frame_callbacks::test_support::app::new_app,
};
use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::{ButtonEvent, MotionEvent},
    reexports::calloop::EventLoop,
    utils::{SERIAL_COUNTER, Serial},
};
use std::{os::unix::net::UnixStream, sync::Arc};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{wl_callback, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_server::Display;

pub(super) mod wire {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports, clippy::all)]
    use wayland_client::{self, protocol::*};
    use wayland_protocols::xdg::shell::client::*;
    mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols::xdg::shell::client::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "../../vendor/xwayland-satellite/protocols/xwayland-satellite-interaction-v1.xml"
        );
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!(
        "../../vendor/xwayland-satellite/protocols/xwayland-satellite-interaction-v1.xml"
    );
}
use wire::xwayland_satellite_interaction_v1::XwaylandSatelliteInteractionV1;

#[derive(Default)]
struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    xdg: Option<xdg_wm_base::XdgWmBase>,
    seats: Vec<wl_seat::WlSeat>,
    manager: Option<XwaylandSatelliteInteractionV1>,
    synced: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(5), qh, ()))
                }
                "xdg_wm_base" => state.xdg = Some(registry.bind(name, version.min(6), qh, ())),
                "wl_seat" => state.seats.push(registry.bind(name, 1, qh, ())),
                "xwayland_satellite_interaction_v1" => {
                    state.manager = Some(registry.bind(name, 1, qh, ()))
                }
                _ => {}
            }
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.synced = true;
    }
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        _: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
        }
    }
}
delegate_noop!(State: wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore wl_pointer::WlPointer);
delegate_noop!(State: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(State: XwaylandSatelliteInteractionV1);

pub(super) struct Fixture {
    pub(super) app: App,
    display: Display<App>,
    connection: Connection,
    queue: EventQueue<State>,
    client: State,
    pub(super) top: xdg_toplevel::XdgToplevel,
    pub(super) index: usize,
    pub(super) serial: Serial,
    _surface: wl_surface::WlSurface,
    _xdg_surface: xdg_surface::XdgSurface,
    _pointer: wl_pointer::WlPointer,
    _event_loop: EventLoop<'static, CalloopData>,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let mut display = Display::new().unwrap();
        let mut dh = display.handle();
        let event_loop = EventLoop::try_new().unwrap();
        let mut app = new_app(&dh, event_loop.handle());
        app.seat_state.new_wl_seat(&dh, "another-seat");
        let (server, peer) = UnixStream::pair().unwrap();
        let server_client = dh
            .insert_client(server, Arc::new(ServerClientState::default()))
            .unwrap();
        app.satellite_interaction.advertise(&dh, server_client.id());
        let connection = Connection::from_socket(peer).unwrap();
        let mut queue = connection.new_event_queue();
        let mut client = State::default();
        connection.display().get_registry(&queue.handle(), ());
        sync(&connection, &mut queue, &mut client, &mut display, &mut app);
        sync(&connection, &mut queue, &mut client, &mut display, &mut app);
        let surface = client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&queue.handle(), ());
        let xdg = client
            .xdg
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &queue.handle(), ());
        let top = xdg.get_toplevel(&queue.handle(), ());
        surface.commit();
        let pointer = client.seats[0].get_pointer(&queue.handle(), ());
        sync(&connection, &mut queue, &mut client, &mut display, &mut app);
        sync(&connection, &mut queue, &mut client, &mut display, &mut app);
        assert_eq!(app.windows.len(), 1);
        app.windows[0].mapped = true;
        app.windows[0].content_bbox_size = (320, 240).into();
        Self {
            app,
            display,
            connection,
            queue,
            client,
            top,
            index: 0,
            serial: 0.into(),
            _surface: surface,
            _xdg_surface: xdg,
            _pointer: pointer,
            _event_loop: event_loop,
        }
    }

    pub(super) fn sync(&mut self) {
        sync(
            &self.connection,
            &mut self.queue,
            &mut self.client,
            &mut self.display,
            &mut self.app,
        );
    }

    pub(super) fn start_move(&mut self) -> Serial {
        let surface = self.app.windows[self.index].surface.wl_surface().unwrap();
        self.app.raw_pointer_location = (30., 30.).into();
        self.app.pointer_location = self.app.raw_pointer_location;
        self.app.pointer.clone().motion(
            &mut self.app,
            Some((surface, (0., 0.).into())),
            &MotionEvent {
                location: (30., 30.).into(),
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::from_millis(1),
            },
        );
        self.serial = SERIAL_COUNTER.next_serial();
        self.app.pressed_pointer_buttons.insert(0x110);
        self.app.pointer.clone().button(
            &mut self.app,
            &ButtonEvent {
                serial: self.serial,
                time: InputTime::from_millis(2),
                button: 0x110,
                state: ButtonState::Pressed,
            },
        );
        self.top._move(&self.client.seats[0], self.serial.into());
        self.sync();
        assert!(
            self.app.drag.is_some(),
            "real XDG request must start an operation"
        );
        self.serial
    }

    pub(super) fn release(&mut self) {
        self.app.pressed_pointer_buttons.clear();
        self.app.pointer.clone().button(
            &mut self.app,
            &ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::from_millis(3),
                button: 0x110,
                state: ButtonState::Released,
            },
        );
        self.sync();
    }

    pub(super) fn cancel(&mut self, top: &xdg_toplevel::XdgToplevel, serial: Serial, seat: usize) {
        self.client
            .manager
            .as_ref()
            .unwrap()
            .cancel(top, &self.client.seats[seat], serial.into());
        self.sync();
    }

    pub(super) fn other_toplevel(&mut self) -> xdg_toplevel::XdgToplevel {
        let surface = self
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ());
        let xdg =
            self.client
                .xdg
                .as_ref()
                .unwrap()
                .get_xdg_surface(&surface, &self.queue.handle(), ());
        let top = xdg.get_toplevel(&self.queue.handle(), ());
        surface.commit();
        self.sync();
        top
    }

    pub(super) fn ordinary_client_cannot_see_manager(&mut self) {
        let (server, peer) = UnixStream::pair().unwrap();
        self.display
            .handle()
            .insert_client(server, Arc::new(ServerClientState::default()))
            .unwrap();
        let connection = Connection::from_socket(peer).unwrap();
        let mut queue = connection.new_event_queue();
        let mut client = State::default();
        connection.display().get_registry(&queue.handle(), ());
        sync(
            &connection,
            &mut queue,
            &mut client,
            &mut self.display,
            &mut self.app,
        );
        assert!(client.manager.is_none());
        assert!(
            client.xdg.is_some(),
            "ordinary globals must still be visible"
        );
    }
}

fn sync(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    client: &mut State,
    display: &mut Display<App>,
    app: &mut App,
) {
    client.synced = false;
    connection.display().sync(&queue.handle(), ());
    connection.flush().unwrap();
    display.dispatch_clients(app).unwrap();
    display.flush_clients().unwrap();
    for _ in 0..16 {
        queue.dispatch_pending(client).unwrap();
        if client.synced {
            return;
        }
        queue.prepare_read().unwrap().read().unwrap();
    }
    panic!("interaction fixture sync was not dispatched");
}
