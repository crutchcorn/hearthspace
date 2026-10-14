//! A real ext-session-lock client. Tests authenticate by owning this protocol
//! object; production uses swaylock/PAM and has no compositor unlock command.
use std::{
    fs::File,
    io::Write,
    os::{fd::AsFd, unix::net::UnixStream},
    path::Path,
    time::{Duration, Instant},
};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface},
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1, ext_session_lock_surface_v1, ext_session_lock_v1,
};

#[derive(Default)]
struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    manager: Option<ext_session_lock_manager_v1::ExtSessionLockManagerV1>,
    outputs: Vec<wl_output::WlOutput>,
    buffers: Vec<(File, wl_buffer::WlBuffer)>,
    locked: bool,
    finished: bool,
}

pub(super) struct LockClient {
    queue: EventQueue<State>,
    state: State,
    lock: ext_session_lock_v1::ExtSessionLockV1,
    surfaces: Vec<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1>,
    _connection: Connection,
}

impl LockClient {
    pub(super) fn connect(socket: &Path) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        let _registry = connection.display().get_registry(&handle, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).unwrap();
        let lock = state
            .manager
            .as_ref()
            .expect("session lock global")
            .lock(&handle, ());
        let surfaces = state
            .outputs
            .iter()
            .map(|output| {
                let surface = state
                    .compositor
                    .as_ref()
                    .unwrap()
                    .create_surface(&handle, ());
                lock.get_lock_surface(&surface, output, &handle, surface.clone())
            })
            .collect();
        Self {
            queue,
            state,
            lock,
            surfaces,
            _connection: connection,
        }
    }

    pub(super) fn wait_locked(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.state.locked {
            self.queue.roundtrip(&mut self.state).unwrap();
            assert!(!self.state.finished, "compositor rejected test lock");
            assert!(
                Instant::now() < deadline,
                "compositor never confirmed lock presentation"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(super) fn unlock(mut self) {
        self.lock.unlock_and_destroy();
        self.queue.roundtrip(&mut self.state).unwrap();
    }

    pub(super) fn destroy_surfaces(&mut self) {
        for surface in self.surfaces.drain(..) {
            surface.destroy();
        }
        // Keep the wl_surfaces and lock object alive, exercising role teardown
        // independently from client disconnection and ordinary surface removal.
        self.queue.roundtrip(&mut self.state).unwrap();
    }

    pub(super) fn wait_rejected(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.state.finished {
            self.queue.roundtrip(&mut self.state).unwrap();
            assert!(
                !self.state.locked,
                "another client took over an existing lock"
            );
            assert!(
                Instant::now() < deadline,
                "second locker was never rejected"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), handle, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, handle, ())),
                "wl_output" => state
                    .outputs
                    .push(registry.bind(name, version.min(3), handle, ())),
                "ext_session_lock_manager_v1" => {
                    state.manager = Some(registry.bind(name, 1, handle, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ext_session_lock_v1::ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_session_lock_v1::Event::Locked => state.locked = true,
            ext_session_lock_v1::Event::Finished => state.finished = true,
            _ => {}
        }
    }
}

impl Dispatch<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, wl_surface::WlSurface>
    for State
{
    fn event(
        state: &mut Self,
        lock_surface: &ext_session_lock_surface_v1::ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        surface: &wl_surface::WlSurface,
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            lock_surface.ack_configure(serial);
            assert!(width > 0 && height > 0 && width <= 16_384 && height <= 16_384);
            let size = width as usize * height as usize * 4;
            let mut file = tempfile::tempfile().unwrap();
            file.set_len(size as u64).unwrap();
            let pixels = [0x12, 0x10, 0x10, 0xff].repeat(width as usize * height as usize);
            file.write_all(&pixels).unwrap();
            let pool =
                state
                    .shm
                    .as_ref()
                    .unwrap()
                    .create_pool(file.as_fd(), size as i32, handle, ());
            let buffer = pool.create_buffer(
                0,
                width as i32,
                height as i32,
                width as i32 * 4,
                wl_shm::Format::Argb8888,
                handle,
                (),
            );
            surface.attach(Some(&buffer), 0, 0);
            surface.damage_buffer(0, 0, width as i32, height as i32);
            surface.commit();
            pool.destroy();
            state.buffers.push((file, buffer));
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_output::WlOutput);
delegate_noop!(State: ignore ext_session_lock_manager_v1::ExtSessionLockManagerV1);
