//! Small protocol fixture: fixed-size shell palettes with real pointer/focus events.
use std::{fs::File, io::Write, os::fd::AsFd, os::unix::net::UnixStream, path::Path};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

#[path = "canvas_map_client/pixels.rs"]
mod pixels;
#[path = "canvas_map_client/decoration.rs"]
mod decoration;
#[path = "canvas_map_client/resize.rs"]
mod resize;

struct Window {
    surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    size: (i32, i32),
    inset: (i32, i32),
    color: [u8; 4],
    rectangle: Option<pixels::ColoredRectangle>,
    alive: bool,
    defer_buffer: bool,
}

#[derive(Default)]
struct State {
    decorations: decoration::State,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    windows: Vec<Window>,
    buffers: Vec<(File, wl_buffer::WlBuffer)>,
    focus: Option<usize>,
    pointer: Option<usize>,
    pointer_xy: (f64, f64),
    presses: Vec<(usize, (f64, f64))>,
    releases: Vec<(usize, (f64, f64))>,
    closed: Vec<usize>,
    keys: Vec<u32>,
    enter_keys: Vec<u32>,
}

pub(super) struct MapClient {
    queue: EventQueue<State>,
    state: State,
    _connection: Connection,
}

impl MapClient {
    pub(super) fn connect(socket: &Path) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        let _registry = connection.display().get_registry(&handle, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).unwrap();
        Self {
            queue,
            state,
            _connection: connection,
        }
    }

    pub(super) fn add(
        &mut self,
        app_id: String,
        size: (i32, i32),
        inset: (i32, i32),
        color: [u8; 4],
    ) -> usize {
        let handle = self.queue.handle();
        let number = self.state.windows.len();
        let surface = self
            .state
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&handle, ());
        let xdg_surface = self
            .state
            .wm_base
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &handle, number);
        let toplevel = xdg_surface.get_toplevel(&handle, number);
        toplevel.set_app_id(app_id);
        self.state.decorations.create(&toplevel, &handle, number);
        toplevel.set_min_size(size.0, size.1);
        toplevel.set_max_size(size.0, size.1);
        self.state.windows.push(Window {
            surface: surface.clone(),
            xdg_surface,
            toplevel,
            size,
            inset,
            color,
            rectangle: None,
            alive: true,
            defer_buffer: false,
        });
        surface.commit();
        self.sync();
        number
    }

    pub(super) fn sync(&mut self) {
        self.queue.roundtrip(&mut self.state).unwrap();
        self.queue.roundtrip(&mut self.state).unwrap();
        self.queue.roundtrip(&mut self.state).unwrap();
    }

    pub(super) fn resize(&mut self, number: usize, size: (i32, i32)) {
        let window = &self.state.windows[number];
        window.toplevel.set_min_size(size.0, size.1);
        window.toplevel.set_max_size(size.0, size.1);
        window.surface.commit();
        self.sync();
    }

    pub(super) fn size(&mut self, number: usize) -> (i32, i32) {
        self.sync();
        self.state.windows[number].size
    }

    pub(super) fn repaint_with_size(&mut self, number: usize, size: (i32, i32)) {
        let window = &mut self.state.windows[number];
        window.size = size;
        window.toplevel.set_min_size(size.0, size.1);
        window.toplevel.set_max_size(size.0, size.1);
        self.state.commit_buffer(number, &self.queue.handle());
        self.sync();
    }

    pub(super) fn repaint_color(&mut self, number: usize, color: [u8; 4]) {
        self.state.windows[number].color = color;
        self.state.windows[number].rectangle = None;
        self.state.commit_buffer(number, &self.queue.handle());
        self.sync();
    }

    pub(super) fn destroy(&mut self, number: usize) {
        let window = &mut self.state.windows[number];
        window.alive = false;
        window.toplevel.destroy();
        window.xdg_surface.destroy();
        window.surface.destroy();
        self.sync();
    }

    pub(super) fn focus(&mut self) -> Option<usize> {
        self.sync();
        self.state.focus
    }
    pub(super) fn presses(&mut self) -> &[(usize, (f64, f64))] {
        self.sync();
        &self.state.presses
    }
    pub(super) fn releases(&mut self) -> &[(usize, (f64, f64))] {
        self.sync();
        &self.state.releases
    }
    pub(super) fn closed(&mut self) -> &[usize] {
        self.sync();
        &self.state.closed
    }
    pub(super) fn keys(&mut self) -> &[u32] {
        self.sync();
        &self.state.keys
    }

    pub(super) fn enter_keys(&mut self) -> &[u32] {
        self.sync();
        &self.state.enter_keys
    }
}

impl State {
    fn commit_buffer(&mut self, number: usize, handle: &QueueHandle<Self>) {
        let window = &self.windows[number];
        if !window.alive {
            return;
        }
        let width = window.size.0 + window.inset.0 * 2;
        let height = window.size.1 + window.inset.1 * 2;
        let size = width as usize * height as usize * 4;
        let mut file = tempfile::tempfile().unwrap();
        file.set_len(size as u64).unwrap();
        file.write_all(&pixels::paint(window)).unwrap();
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
        window.xdg_surface.set_window_geometry(
            window.inset.0,
            window.inset.1,
            window.size.0,
            window.size.1,
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
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind(name, version.min(6), handle, ()))
                }
                "zxdg_decoration_manager_v1" => {
                    state.decorations.bind(registry, name, handle);
                }
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(7), handle, ());
                    seat.get_pointer(handle, ());
                    seat.get_keyboard(handle, ());
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
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer = state
                    .windows
                    .iter()
                    .position(|window| window.surface == surface);
                state.pointer_xy = (surface_x, surface_y);
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => state.pointer_xy = (surface_x, surface_y),
            wl_pointer::Event::Leave { .. } => state.pointer = None,
            wl_pointer::Event::Button {
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } => {
                if let Some(number) = state.pointer {
                    state.presses.push((number, state.pointer_xy));
                }
            }
            wl_pointer::Event::Button {
                state: WEnum::Value(wl_pointer::ButtonState::Released),
                ..
            } => {
                if let Some(number) = state.pointer {
                    state.releases.push((number, state.pointer_xy));
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
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
                state.enter_keys = keys
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| u32::from_ne_bytes(*bytes))
                    .collect();
                state.focus = state
                    .windows
                    .iter()
                    .position(|window| window.surface == surface)
            }
            wl_keyboard::Event::Leave { .. } => state.focus = None,
            wl_keyboard::Event::Key { key, .. } => state.keys.push(key),
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
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if width > 0 {
                    state.windows[*number].size.0 = width;
                }
                if height > 0 {
                    state.windows[*number].size.1 = height;
                }
            }
            xdg_toplevel::Event::Close => state.closed.push(*number),
            _ => {}
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
            if !state.windows[*number].defer_buffer {
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
