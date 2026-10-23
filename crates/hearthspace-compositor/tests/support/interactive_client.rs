//! SHM client for asynchronous client requests and compositor-owned decorations.
use std::{fs::File, io::Write, os::fd::AsFd, os::unix::net::UnixStream, path::Path};

use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_seat, wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::{
    decoration::zv1::client::{zxdg_decoration_manager_v1, zxdg_toplevel_decoration_v1},
    shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

#[path = "interactive_client_events.rs"]
pub(crate) mod events;

struct Window {
    surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    size: (i32, i32),
    inset: (i32, i32),
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
    keyboard: Option<wl_keyboard::WlKeyboard>,
    windows: Vec<Window>,
    buffers: Vec<(File, wl_buffer::WlBuffer)>,
    press_serial: Option<u32>,
    released_buttons: Vec<u32>,
    pointer_events: Vec<wl_pointer::Event>,
    keyboard_events: Vec<wl_keyboard::Event>,
    closed_windows: Vec<usize>,
}

pub(crate) struct InteractiveClient {
    queue: EventQueue<State>,
    state: State,
    _connection: Connection,
}

impl InteractiveClient {
    pub(crate) fn connect(socket: &Path) -> Self {
        Self::connect_with_chrome(socket, false, (0, 0))
    }

    pub(crate) fn connect_server_chrome(socket: &Path) -> Self {
        Self::connect_with_chrome(socket, true, (0, 0))
    }

    pub(crate) fn connect_with_inset(socket: &Path, inset: (i32, i32)) -> Self {
        Self::connect_with_chrome(socket, false, inset)
    }

    fn connect_with_chrome(socket: &Path, server_chrome: bool, inset: (i32, i32)) -> Self {
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
                xdg_surface,
                toplevel,
                size: (240, 160),
                inset,
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

    pub(crate) fn remap(&mut self, number: usize) {
        // Unmap's barriers already acknowledged the fresh initial configure.
        // Reapply role properties cleared by unmapping, then attach a buffer
        // to the same wl_surface and xdg_toplevel.
        let window = &mut self.state.windows[number];
        window.mapped = true;
        window
            .toplevel
            .set_app_id("dev.hearthspace.InteractiveTest".into());
        window
            .toplevel
            .set_title(format!("Interactive fixture {number}"));
        self.state.commit_buffer(number, &self.queue.handle());
        self.sync();
    }

    pub(crate) fn destroy(&mut self, window: usize) {
        let window = &mut self.state.windows[window];
        window.mapped = false;
        window.toplevel.destroy();
        window.xdg_surface.destroy();
        window.surface.destroy();
        self.sync();
    }

    pub(crate) fn take_keyboard_events(&mut self) -> Vec<wl_keyboard::Event> {
        self.sync();
        std::mem::take(&mut self.state.keyboard_events)
    }

    pub(crate) fn released_buttons(&mut self) -> &[u32] {
        self.sync();
        &self.state.released_buttons
    }

    /// Drain ordered pointer events, omitting frame delimiters.
    pub(crate) fn take_pointer_events(&mut self) -> Vec<wl_pointer::Event> {
        self.sync();
        std::mem::take(&mut self.state.pointer_events)
    }

    pub(crate) fn surface(&self, window: usize) -> &wl_surface::WlSurface {
        &self.state.windows[window].surface
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
        self.request_resize_edge(window, serial, xdg_toplevel::ResizeEdge::BottomRight);
    }

    pub(crate) fn request_resize_edge(
        &mut self,
        window: usize,
        serial: u32,
        edge: xdg_toplevel::ResizeEdge,
    ) {
        self.state.windows[window]
            .toplevel
            .resize(self.state.seat.as_ref().unwrap(), serial, edge);
        self.sync();
    }

    pub(crate) fn set_size_limits(&mut self, window: usize, min: (i32, i32), max: (i32, i32)) {
        let window = &self.state.windows[window];
        window.toplevel.set_min_size(min.0, min.1);
        window.toplevel.set_max_size(max.0, max.1);
        window.surface.commit();
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

    pub(crate) fn configuration(&mut self, window: usize) -> (usize, bool) {
        self.sync();
        let window = &self.state.windows[window];
        (window.configure_count, window.resizing)
    }
}

impl State {
    fn commit_buffer(&mut self, number: usize, handle: &QueueHandle<Self>) {
        let window = &self.windows[number];
        let (content_width, content_height) = window.size;
        let (inset_x, inset_y) = window.inset;
        let (width, height) = (content_width + inset_x * 2, content_height + inset_y * 2);
        window
            .xdg_surface
            .set_window_geometry(inset_x, inset_y, content_width, content_height);
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
