//! In-process Smithay server and real Wayland clients, independent of Hearthspace.
use std::{os::unix::net::UnixStream, sync::Arc};

use smithay::{
    backend::input::InputTime,
    input::{
        SeatHandler, SeatState,
        touch::{DownEvent, MotionEvent, TouchHandle, UpEvent},
    },
    wayland::{
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        pointer_constraints::PointerConstraintsHandler,
    },
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{
        wl_callback, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_surface, wl_touch,
    },
};
use wayland_server::{Client, Display, backend::ClientData, protocol::wl_surface::WlSurface};

#[derive(Debug, PartialEq)]
pub enum Event {
    Down(i32, u32),
    Up(i32),
    Motion(i32),
    Frame,
    Cancel,
}

pub struct Server {
    compositor: CompositorState,
    seats: SeatState<Self>,
    surfaces: Vec<WlSurface>,
}

impl SeatHandler for Server {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seats
    }
}

impl PointerConstraintsHandler for Server {}

impl CompositorHandler for Server {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientDataState>().unwrap().0
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        self.surfaces.push(surface.clone());
    }

    fn commit(&mut self, _: &WlSurface) {}
}

smithay::delegate_dispatch2!(Server);

#[derive(Default)]
struct ClientDataState(CompositorClientState);
impl ClientData for ClientDataState {}

#[derive(Default)]
struct ClientState {
    compositor: Option<wl_compositor::WlCompositor>,
    seat: Option<wl_seat::WlSeat>,
    events: Vec<Event>,
    pointer_events: Vec<wl_pointer::Event>,
    synced: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for ClientState {
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
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                _ => {}
            }
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

impl Dispatch<wl_touch::WlTouch, ()> for ClientState {
    fn event(
        state: &mut Self,
        _: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.events.push(match event {
            wl_touch::Event::Down { id, surface, .. } => {
                Event::Down(id, surface.id().protocol_id())
            }
            wl_touch::Event::Up { id, .. } => Event::Up(id),
            wl_touch::Event::Motion { id, .. } => Event::Motion(id),
            wl_touch::Event::Frame => Event::Frame,
            wl_touch::Event::Cancel => Event::Cancel,
            other => panic!("unexpected touch event: {other:?}"),
        });
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
        state.pointer_events.push(event);
    }
}

delegate_noop!(ClientState: ignore wl_compositor::WlCompositor);
delegate_noop!(ClientState: ignore wl_surface::WlSurface);
delegate_noop!(ClientState: ignore wl_seat::WlSeat);

struct WireClient {
    connection: Connection,
    queue: EventQueue<ClientState>,
    state: ClientState,
    surfaces: Vec<wl_surface::WlSurface>,
    targets: Vec<WlSurface>,
}

impl WireClient {
    fn sync(&mut self, display: &mut Display<Server>, server: &mut Server) {
        self.state.synced = false;
        self.connection.display().sync(&self.queue.handle(), ());
        self.connection.flush().unwrap();
        display.dispatch_clients(server).unwrap();
        display.flush_clients().unwrap();
        // Both endpoints are driven here: no process, thread, renderer or timing wait.
        for _ in 0..16 {
            self.queue.dispatch_pending(&mut self.state).unwrap();
            if self.state.synced {
                return;
            }
            self.queue.prepare_read().unwrap().read().unwrap();
        }
        panic!("Wayland sync reply was not dispatched");
    }
}

pub struct Fixture {
    display: Display<Server>,
    pub server: Server,
    pub touch: TouchHandle<Server>,
    clients: Vec<WireClient>,
}

impl Fixture {
    pub fn new() -> Self {
        let display = Display::<Server>::new().unwrap();
        let mut seats = SeatState::new();
        let mut seat = seats.new_wl_seat(&display.handle(), "touch-cancellation");
        let touch = seat.add_touch();
        seat.add_pointer();
        Self {
            server: Server {
                compositor: CompositorState::new::<Server>(&display.handle()),
                seats,
                surfaces: Vec::new(),
            },
            display,
            touch,
            clients: Vec::new(),
        }
    }

    pub fn connect(&mut self) -> usize {
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        self.display
            .handle()
            .insert_client(server_socket, Arc::new(ClientDataState::default()))
            .unwrap();
        let connection = Connection::from_socket(client_socket).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut client = WireClient {
            connection,
            queue,
            state: ClientState::default(),
            surfaces: Vec::new(),
            targets: Vec::new(),
        };
        client.sync(&mut self.display, &mut self.server);
        client.sync(&mut self.display, &mut self.server);
        let qh = client.queue.handle();
        let seat = client.state.seat.as_ref().unwrap();
        seat.get_touch(&qh, ());
        seat.get_pointer(&qh, ());
        let first = self.server.surfaces.len();
        let compositor = client.state.compositor.as_ref().unwrap();
        client.surfaces.push(compositor.create_surface(&qh, ()));
        client.surfaces.push(compositor.create_surface(&qh, ()));
        client.sync(&mut self.display, &mut self.server);
        client.targets = self.server.surfaces[first..].to_vec();
        assert_eq!(client.targets.len(), 2);
        let index = self.clients.len();
        self.clients.push(client);
        index
    }

    pub fn down(&mut self, client: usize, surface: usize, slot: u32) {
        self.touch.down(
            &mut self.server,
            Some((
                self.clients[client].targets[surface].clone(),
                (0.0, 0.0).into(),
            )),
            &DownEvent {
                slot: Some(slot).into(),
                location: (20.0, 30.0).into(),
                serial: (slot + 1).into(),
                time: InputTime::from_millis(10),
            },
        );
    }

    pub fn up(&mut self, slot: u32) {
        self.touch.up(
            &mut self.server,
            &UpEvent {
                slot: Some(slot).into(),
                serial: 100.into(),
                time: InputTime::from_millis(20),
            },
        );
    }

    pub fn motion(&mut self, slot: u32) {
        self.touch.motion(
            &mut self.server,
            None,
            &MotionEvent {
                slot: Some(slot).into(),
                location: (25.0, 35.0).into(),
                time: InputTime::from_millis(15),
            },
        );
    }

    pub fn frame(&mut self) {
        self.touch.frame(&mut self.server);
    }

    pub fn cancel(&mut self) {
        self.touch.cancel(&mut self.server);
    }

    pub fn down_event(&self, client: usize, surface: usize, slot: i32) -> Event {
        Event::Down(
            slot,
            self.clients[client].surfaces[surface].id().protocol_id(),
        )
    }

    pub fn assert_events(&mut self, client: usize, expected: &[Event]) {
        let client = &mut self.clients[client];
        client.sync(&mut self.display, &mut self.server);
        assert_eq!(std::mem::take(&mut client.state.events), expected);
        assert!(
            client.state.pointer_events.is_empty(),
            "touch generated pointer events: {:?}",
            client.state.pointer_events
        );
    }
}
