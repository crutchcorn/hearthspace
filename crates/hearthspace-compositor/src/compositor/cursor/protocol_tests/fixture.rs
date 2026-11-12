use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    sync::Arc,
};

use smithay::{
    backend::input::InputTime, input::pointer::MotionEvent, reexports::calloop::EventLoop,
    utils::SERIAL_COUNTER,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1, wp_cursor_shape_manager_v1,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_server::{Client, Display, protocol::wl_surface::WlSurface};

use crate::compositor::{
    App, CalloopData, client::ClientState as ServerClientState,
    frame_callbacks::test_support::app::new_app,
};

pub(super) struct Fixture {
    pub(super) app: App,
    pub(super) display: Display<App>,
    pub(super) client: Peer,
    _event_loop: EventLoop<'static, CalloopData>,
}

pub(super) struct Peer {
    pub(super) state: State,
    pub(super) connection: Connection,
    pub(super) queue: EventQueue<State>,
    server: Client,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let display = Display::new().unwrap();
        let event_loop = EventLoop::try_new().unwrap();
        let app = new_app(&display.handle(), event_loop.handle());
        let client = Peer::new(&display);
        let mut fixture = Self {
            app,
            display,
            client,
            _event_loop: event_loop,
        };
        fixture.sync();
        fixture.sync();
        fixture
    }

    pub(super) fn sync(&mut self) {
        self.client.sync(&mut self.display, &mut self.app);
    }

    pub(super) fn focus(&mut self, surface: Option<&wl_surface::WlSurface>) -> u32 {
        let focus = surface.map(|s| self.client.server_surface(&self.display, s));
        self.focus_server(focus)
    }

    pub(super) fn focus_server(&mut self, surface: Option<WlSurface>) -> u32 {
        let serial = SERIAL_COUNTER.next_serial();
        self.app.pointer.clone().motion(
            &mut self.app,
            surface.map(|s| (s, (0., 0.).into())),
            &MotionEvent {
                location: (1., 1.).into(),
                serial,
                time: InputTime::from_millis(1),
            },
        );
        self.app.pointer.clone().frame(&mut self.app);
        self.sync();
        serial.into()
    }

    pub(super) fn shape(&self) -> wp_cursor_shape_device_v1::WpCursorShapeDeviceV1 {
        self.client
            .state
            .manager
            .as_ref()
            .expect("cursor-shape global")
            .get_pointer(
                self.client.state.pointer.as_ref().unwrap(),
                &self.client.queue.handle(),
                (),
            )
    }
}

impl Peer {
    pub(super) fn new(display: &Display<App>) -> Self {
        let (server, client) = UnixStream::pair().unwrap();
        let server = display
            .handle()
            .insert_client(server, Arc::new(ServerClientState::default()))
            .unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        Self {
            state: State::default(),
            connection,
            queue,
            server,
        }
    }

    pub(super) fn surface(&self) -> wl_surface::WlSurface {
        self.state
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ())
    }

    pub(super) fn map_window(
        &mut self,
        display: &mut Display<App>,
        app: &mut App,
    ) -> wl_surface::WlSurface {
        let surface = self.surface();
        let xdg =
            self.state
                .shell
                .as_ref()
                .unwrap()
                .get_xdg_surface(&surface, &self.queue.handle(), ());
        let _toplevel = xdg.get_toplevel(&self.queue.handle(), ());
        surface.commit();
        self.sync(display, app);
        self.sync(display, app);
        let file = tempfile::tempfile().unwrap();
        file.set_len(200 * 100 * 4).unwrap();
        let pool = self.state.shm.as_ref().unwrap().create_pool(
            file.as_fd(),
            200 * 100 * 4,
            &self.queue.handle(),
            (),
        );
        let buffer = pool.create_buffer(
            0,
            200,
            100,
            800,
            wl_shm::Format::Argb8888,
            &self.queue.handle(),
            (),
        );
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        self.sync(display, app);
        let window = app.windows.iter_mut().next().unwrap();
        window.position = crate::geometry::CanvasPoint { x: 100, y: 100 };
        window.decoration = crate::compositor::WindowDecoration::ClientSide;
        assert!(window.mapped);
        surface
    }

    pub(super) fn server_surface(
        &self,
        display: &Display<App>,
        surface: &wl_surface::WlSurface,
    ) -> WlSurface {
        self.server
            .object_from_protocol_id(&display.handle(), surface.id().protocol_id())
            .unwrap()
    }

    pub(super) fn sync(&mut self, display: &mut Display<App>, app: &mut App) {
        self.state.synced = false;
        self.connection.display().sync(&self.queue.handle(), ());
        self.connection.flush().unwrap();
        display.dispatch_clients(app).unwrap();
        display.flush_clients().unwrap();
        while !self.state.synced {
            self.queue.dispatch_pending(&mut self.state).unwrap();
            if !self.state.synced {
                self.queue.prepare_read().unwrap().read().unwrap();
            }
        }
        assert!(self.connection.protocol_error().is_none());
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) compositor: Option<wl_compositor::WlCompositor>,
    pub(super) pointer: Option<wl_pointer::WlPointer>,
    pub(super) manager: Option<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1>,
    pub(super) enter: Option<u32>,
    shell: Option<xdg_wm_base::XdgWmBase>,
    shm: Option<wl_shm::WlShm>,
    synced: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(5), queue, ()))
                }
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(7), queue, ());
                    state.pointer = Some(seat.get_pointer(queue, ()));
                }
                "wp_cursor_shape_manager_v1" => {
                    assert_eq!(version, 2);
                    state.manager = Some(registry.bind(name, 2, queue, ()));
                }
                "xdg_wm_base" => state.shell = Some(registry.bind(name, version.min(6), queue, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, queue, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { serial, .. } => state.enter = Some(serial),
            wl_pointer::Event::Leave { .. } => state.enter = None,
            _ => {}
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

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(State: ignore wp_cursor_shape_manager_v1::WpCursorShapeManagerV1);
delegate_noop!(State: ignore wp_cursor_shape_device_v1::WpCursorShapeDeviceV1);

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        shell: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            shell.pong(serial);
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
