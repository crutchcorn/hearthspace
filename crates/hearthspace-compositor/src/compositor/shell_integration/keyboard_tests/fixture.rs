//! Renderer-free App setup reused from the callback tests, with a keyboard client.
//! Focus is fixture input; key filtering, modifier state and wire delivery are real.
use std::{os::unix::net::UnixStream, sync::Arc};

use hearthspace_ipc::command::ShellCommand;
use smithay::{reexports::calloop::EventLoop, utils::SERIAL_COUNTER};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop,
    protocol::{wl_callback, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_surface},
};
use wayland_server::{Client, Display, protocol::wl_surface::WlSurface};

use crate::compositor::{
    App, CalloopData, client::ClientState as ServerClientState,
    frame_callbacks::test_support::app::new_app, shell_integration::ControlReply,
};

pub(super) struct Fixture {
    pub(super) app: App,
    pub(super) events: ClientState,
    display: Display<App>,
    server_client: Client,
    connection: Connection,
    queue: EventQueue<ClientState>,
    _event_loop: EventLoop<'static, CalloopData>,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let display = Display::new().unwrap();
        let mut handle = display.handle();
        let event_loop = EventLoop::try_new().unwrap();
        let app = new_app(&handle, event_loop.handle());
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        let server_client = handle
            .insert_client(server_socket, Arc::new(ServerClientState::default()))
            .unwrap();
        let connection = Connection::from_socket(client_socket).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut fixture = Self {
            app,
            events: ClientState::default(),
            display,
            server_client,
            connection,
            queue,
            _event_loop: event_loop,
        };
        fixture.sync();
        fixture.sync();
        assert!(fixture.events.keyboard.is_some());
        let surface = fixture.surface();
        fixture.focus(&surface);
        fixture.app.needs_redraw = false;
        fixture
    }

    pub(super) fn action(&mut self, command: ShellCommand) {
        assert!(matches!(
            self.app.run_control_action(command),
            ControlReply::Ok
        ));
    }

    pub(super) fn surface(&mut self) -> wl_surface::WlSurface {
        let surface = self
            .events
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ());
        self.sync();
        surface
    }

    pub(super) fn focus(&mut self, surface: &wl_surface::WlSurface) {
        let server_surface: WlSurface = self
            .server_client
            .object_from_protocol_id(&self.display.handle(), surface.id().protocol_id())
            .unwrap();
        self.app.keyboard.clone().set_focus(
            &mut self.app,
            Some(server_surface.into()),
            SERIAL_COUNTER.next_serial(),
        );
        self.sync();
        assert_eq!(self.events.focus.as_ref(), Some(surface));
    }

    pub(super) fn take_keys(&mut self) -> Vec<(u32, wl_keyboard::KeyState)> {
        std::mem::take(&mut self.events.keys)
    }

    pub(super) fn sync(&mut self) {
        // Both endpoints are dispatched here, as in the existing App protocol fixtures.
        // No compositor loop, renderer, service workers or sleeping is needed.
        self.events.synced = false;
        self.connection.display().sync(&self.queue.handle(), ());
        self.connection.flush().unwrap();
        self.display.dispatch_clients(&mut self.app).unwrap();
        self.display.flush_clients().unwrap();
        for _ in 0..16 {
            self.queue.dispatch_pending(&mut self.events).unwrap();
            if self.events.synced {
                assert!(self.connection.protocol_error().is_none());
                return;
            }
            self.queue.prepare_read().unwrap().read().unwrap();
        }
        panic!("Wayland keyboard test sync reply was not dispatched");
    }
}

#[derive(Default)]
pub(super) struct ClientState {
    compositor: Option<wl_compositor::WlCompositor>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    focus: Option<wl_surface::WlSurface>,
    pub(super) enter_keys: Vec<u8>,
    pub(super) depressed_modifiers: Option<u32>,
    keys: Vec<(u32, wl_keyboard::KeyState)>,
    synced: bool,
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
                    state.compositor = Some(registry.bind(name, version.min(5), queue, ()));
                }
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(7), queue, ());
                    state.keyboard = Some(seat.get_keyboard(queue, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { surface, keys, .. } => {
                state.focus = Some(surface);
                state.enter_keys = keys;
            }
            wl_keyboard::Event::Leave { .. } => state.focus = None,
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(phase),
                ..
            } => {
                state.keys.push((key, phase));
            }
            wl_keyboard::Event::Modifiers { mods_depressed, .. } => {
                state.depressed_modifiers = Some(mods_depressed);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for ClientState {
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

delegate_noop!(ClientState: ignore wl_compositor::WlCompositor);
delegate_noop!(ClientState: ignore wl_surface::WlSurface);
delegate_noop!(ClientState: ignore wl_seat::WlSeat);
