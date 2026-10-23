//! Real commits must retire CSD chrome when a mapped subsurface joins the tree.
use super::*;
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorHandler, CompositorState, get_parent},
        shm::{ShmHandler, ShmState},
    },
};
use std::{
    fs::File,
    os::{fd::AsFd, unix::net::UnixStream},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_subcompositor,
        wl_subsurface, wl_surface,
    },
};
use wayland_server::{Client, Display, Resource, protocol::wl_surface::WlSurface};

struct ServerState {
    compositor: CompositorState,
    shm: ShmState,
    surfaces: Vec<WlSurface>,
}

impl CompositorHandler for ServerState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<crate::compositor::client::ClientState>()
            .unwrap()
            .compositor_state
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        self.surfaces.push(surface.clone());
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
    }
}

impl BufferHandler for ServerState {
    fn buffer_destroyed(&mut self, _: &wayland_server::protocol::wl_buffer::WlBuffer) {}
}

impl ShmHandler for ServerState {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}

smithay::delegate_dispatch2!(ServerState);

#[derive(Default)]
struct Events {
    compositor: Option<wl_compositor::WlCompositor>,
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    shm: Option<wl_shm::WlShm>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Events {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => state.compositor = Some(registry.bind(name, 1, handle, ())),
                "wl_subcompositor" => {
                    state.subcompositor = Some(registry.bind(name, 1, handle, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, handle, ())),
                _ => {}
            }
        }
    }
}

delegate_noop!(Events: ignore wl_compositor::WlCompositor);
delegate_noop!(Events: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(Events: ignore wl_surface::WlSurface);
delegate_noop!(Events: ignore wl_subsurface::WlSubsurface);
delegate_noop!(Events: ignore wl_shm::WlShm);
delegate_noop!(Events: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Events: ignore wl_buffer::WlBuffer);

struct TreeClient {
    _connection: Connection,
    queue: EventQueue<Events>,
    events: Events,
    root: wl_surface::WlSurface,
    child: wl_surface::WlSurface,
    child_buffer: wl_buffer::WlBuffer,
    _subsurface: wl_subsurface::WlSubsurface,
    _root_buffer: wl_buffer::WlBuffer,
    _storage: File,
}

impl TreeClient {
    fn new(socket: UnixStream) -> Self {
        let connection = Connection::from_socket(socket).unwrap();
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        connection.display().get_registry(&handle, ());
        let mut events = Events::default();
        queue.roundtrip(&mut events).unwrap();
        let compositor = events.compositor.as_ref().unwrap();
        let root = compositor.create_surface(&handle, ());
        let child = compositor.create_surface(&handle, ());
        let subsurface =
            events
                .subcompositor
                .as_ref()
                .unwrap()
                .get_subsurface(&child, &root, &handle, ());
        subsurface.set_position(4, 4);
        let storage = tempfile::tempfile().unwrap();
        storage.set_len(64 * 48 * 4).unwrap();
        let pool =
            events
                .shm
                .as_ref()
                .unwrap()
                .create_pool(storage.as_fd(), 64 * 48 * 4, &handle, ());
        let root_buffer =
            pool.create_buffer(0, 64, 48, 64 * 4, wl_shm::Format::Xrgb8888, &handle, ());
        let child_buffer =
            pool.create_buffer(0, 8, 8, 64 * 4, wl_shm::Format::Xrgb8888, &handle, ());
        pool.destroy();
        root.attach(Some(&root_buffer), 0, 0);
        root.damage(0, 0, 64, 48);
        root.commit();
        queue.roundtrip(&mut events).unwrap();
        Self {
            _connection: connection,
            queue,
            events,
            root,
            child,
            child_buffer,
            _subsurface: subsurface,
            _root_buffer: root_buffer,
            _storage: storage,
        }
    }

    fn sync(&mut self) {
        self.queue.roundtrip(&mut self.events).unwrap();
    }
}

fn dispatch_client<T: Send + 'static>(
    display: &mut Display<ServerState>,
    state: &mut ServerState,
    action: impl FnOnce() -> T + Send + 'static,
) -> T {
    // Same bounded in-process dispatch pattern as gtk_shell/seat_tests.rs.
    let client = thread::spawn(action);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !client.is_finished() {
        assert!(
            Instant::now() < deadline,
            "surface-tree roundtrip timed out"
        );
        display.dispatch_clients(state).unwrap();
        display.flush_clients().unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    client.join().unwrap()
}

fn frame_for_root(
    renderer: &mut GlesRenderer,
    cache: &mut WindowFrameCache,
    root: &WlSurface,
) -> Option<WindowFrameElements> {
    let geometry = Rectangle::from_size((64, 48).into());
    let kind = FrameKind::for_client(root, geometry);
    cache.elements_with_opacity(
        renderer,
        1,
        Rectangle::new((80, 60).into(), geometry.size),
        1.,
        true,
        false,
        kind,
        false,
        false,
        1.,
    )
}

#[test]
fn mapped_subsurface_retires_client_frame_until_its_committed_unmap() {
    let (server_socket, client_socket) = UnixStream::pair().unwrap();
    let mut display = Display::<ServerState>::new().unwrap();
    let mut handle = display.handle();
    handle
        .insert_client(
            server_socket,
            Arc::new(crate::compositor::client::ClientState::default()),
        )
        .unwrap();
    let mut state = ServerState {
        compositor: CompositorState::new::<ServerState>(&handle),
        shm: ShmState::new::<ServerState>(&handle, vec![]),
        surfaces: Vec::new(),
    };
    let client = dispatch_client(&mut display, &mut state, move || {
        TreeClient::new(client_socket)
    });
    assert_eq!(state.surfaces.len(), 2);
    let root = state.surfaces[0].clone();
    assert_eq!(root.id().protocol_id(), client.root.id().protocol_id());
    assert_eq!(get_parent(&state.surfaces[1]), Some(root.clone()));
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let first = frame_for_root(&mut renderer, &mut cache, &root)
        .expect("an unmapped child must not hide the opaque root's frame");

    let client = dispatch_client(&mut display, &mut state, move || {
        let mut client = client;
        client.child.attach(Some(&client.child_buffer), 0, 0);
        client.child.damage(0, 0, 8, 8);
        client.child.commit();
        client.sync();
        client
    });
    assert!(
        frame_for_root(&mut renderer, &mut cache, &root).is_some(),
        "a synchronized child commit stays pending until its parent commits"
    );

    let client = dispatch_client(&mut display, &mut state, move || {
        let mut client = client;
        client.root.commit();
        client.sync();
        client
    });
    assert!(
        frame_for_root(&mut renderer, &mut cache, &root).is_none(),
        "the real mapped child must suppress both rim and shadow"
    );

    let client = dispatch_client(&mut display, &mut state, move || {
        let mut client = client;
        client.child.attach(None, 0, 0);
        client.child.commit();
        client.sync();
        client
    });
    assert!(
        frame_for_root(&mut renderer, &mut cache, &root).is_none(),
        "pending child unmap must not restore chrome prematurely"
    );

    let _client = dispatch_client(&mut display, &mut state, move || {
        let mut client = client;
        client.root.commit();
        client.sync();
        client
    });
    assert_eq!(
        get_parent(&state.surfaces[1]),
        Some(root.clone()),
        "the child remains in the tree after its buffer is detached"
    );
    let restored = frame_for_root(&mut renderer, &mut cache, &root)
        .expect("the frame must return after the mapped child disappears");
    assert_eq!(restored.rim.id(), first.rim.id());
    assert_eq!(restored.rim.current_commit(), first.rim.current_commit());
    assert_eq!(restored.shadow.id(), first.shadow.id());
    assert!(restored.clip.is_none());
    assert!(restored.header.is_none());
}
