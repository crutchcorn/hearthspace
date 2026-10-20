//! SHM client for asynchronous client requests and compositor-owned decorations.
use std::{fs::File, io::Write, os::fd::AsFd, os::unix::net::UnixStream, path::Path};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::{
    decoration::zv1::client::{zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

struct Window {
    surface: wl_surface::WlSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    size: (i32, i32),
    configure_count: usize,
    resizing: bool,
    mapped: bool,
}

#[derive(Default)]
struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    decoration_manager: Option<zxdg_decoration_manager_v1::ZxdgDecorationManagerV1>,
    decorations: Vec<zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1>,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    windows: Vec<Window>,
    buffers: Vec<(File, wl_buffer::WlBuffer)>,
    press_serial: Option<u32>,
    released_buttons: Vec<u32>,
    closed_windows: Vec<usize>,
}

pub(crate) struct InteractiveClient {
    queue: EventQueue<State>,
    state: State,
    _connection: Connection,
}

impl InteractiveClient {
    pub(crate) fn connect(socket: &Path) -> Self {
        Self::connect_with_chrome(socket, false)
    }

    pub(crate) fn connect_server_chrome(socket: &Path) -> Self {
        Self::connect_with_chrome(socket, true)
    }

    fn connect_with_chrome(socket: &Path, server_chrome: bool) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        let _registry = connection.display().get_registry(&handle, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).unwrap();
        for number in 0..2 {
            let surface = state
                .compositor
                .as_ref()
                .unwrap()
                .create_surface(&handle, ());
            let xdg_surface = state
                .wm_base
                .as_ref()
                .unwrap()
                .get_xdg_surface(&surface, &handle, number);
            let toplevel = xdg_surface.get_toplevel(&handle, number);
            if server_chrome {
                let decoration = state
                    .decoration_manager
                    .as_ref()
                    .unwrap()
                    .get_toplevel_decoration(&toplevel, &handle, ());
                decoration.set_mode(zxdg_toplevel_decoration_v1::Mode::ServerSide);
                state.decorations.push(decoration);
            }
            toplevel.set_app_id("dev.hearthspace.InteractiveTest".into());
            toplevel.set_title(format!("Interactive fixture {number}"));
            state.windows.push(Window {
                surface: surface.clone(),
                toplevel,
                size: (240, 160),
                configure_count: 0,
                resizing: false,
                mapped: true,
            });
            surface.commit();
        }
        let mut client = Self {
            queue,
            state,
            _connection: connection,
        };
        client.sync();
        client
    }

    /// Two barriers also process commits queued while dispatching configure events.
    pub(crate) fn sync(&mut self) {
        self.queue.roundtrip(&mut self.state).unwrap();
        self.queue.roundtrip(&mut self.state).unwrap();
    }

    pub(crate) fn press_serial(&mut self) -> u32 {
        self.sync();
        self.state
            .press_serial
            .expect("client received pointer press")
    }

    pub(crate) fn received_pointer_press(&mut self) -> bool {
        self.sync();
        self.state.press_serial.is_some()
    }

    pub(crate) fn unmap(&mut self, window: usize) {
        let window = &mut self.state.windows[window];
        window.mapped = false;
        window.surface.attach(None, 0, 0);
        window.surface.commit();
        self.sync();
    }

    pub(crate) fn released_buttons(&mut self) -> &[u32] {
        self.sync();
        &self.state.released_buttons
    }

    pub(crate) fn closed_windows(&mut self) -> &[usize] {
        self.sync();
        &self.state.closed_windows
    }

    pub(crate) fn request_move(&mut self, window: usize, serial: u32) {
        self.state.windows[window]
            .toplevel
            ._move(self.state.seat.as_ref().unwrap(), serial);
        self.sync();
    }

    pub(crate) fn request_resize(&mut self, window: usize, serial: u32) {
        self.state.windows[window].toplevel.resize(
            self.state.seat.as_ref().unwrap(),
            serial,
            xdg_toplevel::ResizeEdge::BottomRight,
        );
        self.sync();
    }

    pub(super) fn request_restore(&mut self, window: usize) {
        self.state.windows[window].toplevel.unset_fullscreen();
        self.state.windows[window].toplevel.unset_maximized();
        self.sync();
    }

    pub(crate) fn request_fullscreen(&mut self, window: usize) {
        self.state.windows[window].toplevel.set_fullscreen(None);
        self.sync();
    }

    pub(super) fn configuration(&mut self, window: usize) -> (usize, bool) {
        self.sync();
        let window = &self.state.windows[window];
        (window.configure_count, window.resizing)
    }
}

impl State {
    fn commit_buffer(&mut self, number: usize, handle: &QueueHandle<Self>) {
        let window = &self.windows[number];
        let (width, height) = window.size;
        assert!(width > 0 && height > 0 && width <= 4096 && height <= 4096);
        let size = width as usize * height as usize * 4;
        let mut file = tempfile::tempfile().unwrap();
        file.set_len(size as u64).unwrap();
        file.write_all(&[0x66, 0xcc, 0x00, 0xff].repeat(width as usize * height as usize))
            .unwrap();
        let pool = self
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), size as i32, handle, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Argb8888,
            handle,
            (),
        );
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, width, height);
        window.surface.commit();
        pool.destroy();
        self.buffers.push((file, buffer));
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
                "zxdg_decoration_manager_v1" => {
                    state.decoration_manager = Some(registry.bind(name, 1, handle, ()));
                }
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind(name, version.min(6), handle, ()))
                }
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(7), handle, ());
                    state.pointer = Some(seat.get_pointer(handle, ()));
                    state.seat = Some(seat);
                }
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
            wl_pointer::Event::Button {
                serial,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } => state.press_serial = Some(serial),
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(wl_pointer::ButtonState::Released),
                ..
            } => state.released_buttons.push(button),
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        object: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            object.pong(serial);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, usize> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        number: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Close = event {
            state.closed_windows.push(*number);
            return;
        }
        if let xdg_toplevel::Event::Configure {
            width,
            height,
            states,
        } = event
        {
            state.windows[*number].resizing =
                states.as_chunks::<4>().0.iter().any(|bytes| {
                    u32::from_ne_bytes(*bytes) == xdg_toplevel::State::Resizing as u32
                });
            if width > 0 {
                state.windows[*number].size.0 = width;
            }
            if height > 0 {
                state.windows[*number].size.1 = height;
            }
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, usize> for State {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        number: &usize,
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.windows[*number].configure_count += 1;
            if state.windows[*number].mapped {
                state.commit_buffer(*number, handle);
            }
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore zxdg_decoration_manager_v1::ZxdgDecorationManagerV1);
delegate_noop!(State: ignore zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1);
