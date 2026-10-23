//! Exercise the titlebar seat gate with real resources from one Wayland client.

use std::{
    os::unix::net::UnixStream,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use smithay::{
    input::{SeatHandler, SeatState},
    wayland::{
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        pointer_constraints::PointerConstraintsHandler,
    },
};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_registry, wl_seat},
};
use wayland_server::{
    Client, Display, Resource, backend::ClientData, protocol::wl_surface::WlSurface,
};

use super::seat_matches;

struct SeatTestState {
    seats: SeatState<Self>,
    compositor: CompositorState,
}

impl SeatHandler for SeatTestState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seats
    }
}

impl PointerConstraintsHandler for SeatTestState {}

impl CompositorHandler for SeatTestState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<SeatClientData>().unwrap().0
    }

    fn commit(&mut self, _: &WlSurface) {}
}

smithay::delegate_dispatch2!(SeatTestState);

#[derive(Default)]
struct SeatClientData(CompositorClientState);

impl ClientData for SeatClientData {}

#[derive(Default)]
struct ClientSeats(Vec<wl_seat::WlSeat>);

impl Dispatch<wl_registry::WlRegistry, ()> for ClientSeats {
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
            && interface == "wl_seat"
        {
            // Distinct resources for the same logical seat must both be valid.
            state.0.push(registry.bind(name, version.min(1), queue, ()));
            state.0.push(registry.bind(name, version.min(1), queue, ()));
        }
    }
}

delegate_noop!(ClientSeats: ignore wl_seat::WlSeat);

#[test]
fn titlebar_seat_gate_rejects_another_seat_from_the_same_client() {
    let (server_socket, client_socket) = UnixStream::pair().unwrap();
    let mut display = Display::<SeatTestState>::new().unwrap();
    let mut handle = display.handle();
    let client = handle
        .insert_client(server_socket, Arc::new(SeatClientData::default()))
        .unwrap();
    let mut state = SeatTestState {
        seats: SeatState::new(),
        compositor: CompositorState::new::<SeatTestState>(&handle),
    };
    let expected = state.seats.new_wl_seat(&handle, "titlebar-seat");
    let other = state.seats.new_wl_seat(&handle, "other-seat");

    let client_thread = thread::spawn(move || {
        let connection = Connection::from_socket(client_socket).unwrap();
        let mut queue = connection.new_event_queue::<ClientSeats>();
        let mut seats = ClientSeats::default();
        connection.display().get_registry(&queue.handle(), ());
        queue.roundtrip(&mut seats).unwrap();
        // Bind requests are queued while receiving the registry globals.
        queue.roundtrip(&mut seats).unwrap();
        assert_eq!(
            seats.0.len(),
            4,
            "both advertised seats must be bound twice"
        );
        (connection, seats)
    });

    let deadline = Instant::now() + Duration::from_secs(5);
    while !client_thread.is_finished() {
        assert!(
            Instant::now() < deadline,
            "seat binding roundtrips timed out"
        );
        display.dispatch_clients(&mut state).unwrap();
        display.flush_clients().unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    // Keep the client connection and proxies alive through the gate assertions.
    let (_connection, _client_seats) = client_thread.join().unwrap();
    let own_resources = expected.client_seats(&client);
    let other_resources = other.client_seats(&client);
    assert_eq!(own_resources.len(), 2);
    assert_eq!(other_resources.len(), 2);
    assert_ne!(own_resources[0].id(), own_resources[1].id());
    assert!(
        own_resources[0]
            .id()
            .same_client_as(&other_resources[0].id())
    );

    for resource in &own_resources {
        assert!(resource.is_alive());
        assert!(
            seat_matches(resource, &expected),
            "every binding of the expected seat must be accepted"
        );
    }
    for resource in &other_resources {
        assert!(resource.is_alive());
        assert!(seat_matches(resource, &other));
        assert!(
            !seat_matches(resource, &expected),
            "sharing a client with the expected seat must not authorize a titlebar gesture"
        );
    }
}
