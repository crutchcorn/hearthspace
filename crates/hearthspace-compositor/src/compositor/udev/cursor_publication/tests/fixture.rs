//! Real App commits and callback events, without a renderer or native session.
use std::{
    fs::File,
    os::{fd::AsFd, unix::net::UnixStream},
    sync::Arc,
};

use smithay::{input::pointer::MotionEvent, reexports::calloop::EventLoop, utils::SERIAL_COUNTER};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
    },
};
use wayland_server::{Client, Display, protocol::wl_surface::WlSurface};

use crate::compositor::{
    App, CalloopData, client::ClientState as ServerClientState,
    frame_callbacks::test_support::app::new_app,
};

pub(super) struct Fixture {
    pub(super) app: App,
    display: Display<App>,
    server_client: Client,
    connection: Connection,
    queue: EventQueue<ClientState>,
    client: ClientState,
    files: Vec<File>,
    _event_loop: EventLoop<'static, CalloopData>,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let display = Display::new().unwrap();
        let mut handle = display.handle();
        let event_loop = EventLoop::try_new().unwrap();
        let app = new_app(&handle, event_loop.handle());
        let (server, client) = UnixStream::pair().unwrap();
        let server_client = handle
            .insert_client(server, Arc::new(ServerClientState::default()))
            .unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut fixture = Self {
            app,
            display,
            server_client,
            connection,
            queue,
            client: ClientState::default(),
            files: Vec::new(),
            _event_loop: event_loop,
        };
        fixture.sync();
        fixture.sync();
        let focus = fixture.surface();
        let server_focus = fixture.server_surface(&focus);
        fixture.app.pointer.clone().motion(
            &mut fixture.app,
            Some((server_focus, (0., 0.).into())),
            &MotionEvent {
                location: (1., 1.).into(),
                serial: SERIAL_COUNTER.next_serial(),
                time: smithay::backend::input::InputTime::from_millis(1),
            },
        );
        fixture.app.pointer.clone().frame(&mut fixture.app);
        fixture.sync();
        assert!(fixture.client.enter_serial.is_some());
        fixture
    }

    pub(super) fn surface(&mut self) -> wl_surface::WlSurface {
        let surface = self
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ());
        self.sync();
        surface
    }

    fn server_surface(&self, surface: &wl_surface::WlSurface) -> WlSurface {
        self.server_client
            .object_from_protocol_id(&self.display.handle(), surface.id().protocol_id())
            .unwrap()
    }

    pub(super) fn cursor(&mut self, surface: Option<&wl_surface::WlSurface>) {
        self.client.pointer.as_ref().unwrap().set_cursor(
            self.client.enter_serial.unwrap(),
            surface,
            0,
            0,
        );
        self.sync();
        assert_eq!(
            self.app.cursor_surface,
            surface.map(|s| self.server_surface(s))
        );
    }

    pub(super) fn child(&mut self, parent: &wl_surface::WlSurface) -> wl_surface::WlSurface {
        let child = self.surface();
        let role = self.client.subcompositor.as_ref().unwrap().get_subsurface(
            &child,
            parent,
            &self.queue.handle(),
            (),
        );
        role.set_desync();
        parent.commit();
        self.sync();
        child
    }

    pub(super) fn commit(
        &mut self,
        surface: &wl_surface::WlSurface,
        pixel: Option<u32>,
        callback: u32,
    ) {
        if let Some(pixel) = pixel {
            use std::os::unix::fs::FileExt;
            let file = tempfile::tempfile().unwrap();
            file.set_len(4).unwrap();
            file.write_all_at(&pixel.to_ne_bytes(), 0).unwrap();
            let pool = self.client.shm.as_ref().unwrap().create_pool(
                file.as_fd(),
                4,
                &self.queue.handle(),
                (),
            );
            let buffer = pool.create_buffer(
                0,
                1,
                1,
                4,
                wl_shm::Format::Argb8888,
                &self.queue.handle(),
                (),
            );
            pool.destroy();
            surface.attach(Some(&buffer), 0, 0);
            surface.damage_buffer(0, 0, 1, 1);
            self.files.push(file);
        }
        surface.frame(&self.queue.handle(), Some(callback));
        surface.commit();
        self.sync();
    }

    pub(super) fn assert_done(&mut self, expected: &[(u32, u32)]) {
        self.sync();
        let mut actual = std::mem::take(&mut self.client.done);
        let mut expected = expected.to_vec();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected);
    }

    pub(super) fn sync(&mut self) {
        self.client.synced = false;
        self.connection
            .display()
            .sync(&self.queue.handle(), None::<u32>);
        self.connection.flush().unwrap();
        self.display.dispatch_clients(&mut self.app).unwrap();
        self.display.flush_clients().unwrap();
        for _ in 0..16 {
            self.queue.dispatch_pending(&mut self.client).unwrap();
            if self.client.synced {
                assert!(self.connection.protocol_error().is_none());
                return;
            }
            self.queue.prepare_read().unwrap().read().unwrap();
        }
        panic!("cursor callback fixture sync reply was not dispatched");
    }
}

#[derive(Default)]
struct ClientState {
    compositor: Option<wl_compositor::WlCompositor>,
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    shm: Option<wl_shm::WlShm>,
    pointer: Option<wl_pointer::WlPointer>,
    enter_serial: Option<u32>,
    synced: bool,
    done: Vec<(u32, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for ClientState {
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
                "wl_subcompositor" => state.subcompositor = Some(registry.bind(name, 1, queue, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, queue, ())),
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(7), queue, ());
                    state.pointer = Some(seat.get_pointer(queue, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Enter { serial, .. } = event {
            state.enter_serial = Some(serial);
        }
    }
}

impl Dispatch<wl_callback::WlCallback, Option<u32>> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        callback: &Option<u32>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { callback_data } = event {
            if let Some(callback) = callback {
                state.done.push((*callback, callback_data));
            } else {
                state.synced = true;
            }
        }
    }
}

delegate_noop!(ClientState: ignore wl_compositor::WlCompositor);
delegate_noop!(ClientState: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(ClientState: ignore wl_subsurface::WlSubsurface);
delegate_noop!(ClientState: ignore wl_surface::WlSurface);
delegate_noop!(ClientState: ignore wl_shm::WlShm);
delegate_noop!(ClientState: ignore wl_shm_pool::WlShmPool);
delegate_noop!(ClientState: ignore wl_buffer::WlBuffer);
delegate_noop!(ClientState: ignore wl_seat::WlSeat);
