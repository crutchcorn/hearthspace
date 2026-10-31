//! Reuse the service-free App and persistent surfaceless GLES test pattern.
use std::{
    io::Write,
    os::{fd::AsFd, unix::net::UnixStream},
    sync::Arc,
};

use smithay::{
    backend::{
        allocator::Fourcc,
        egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
        input::InputTime,
        renderer::{
            Bind, Offscreen,
            damage::OutputDamageTracker,
            gles::{GlesRenderbuffer, GlesRenderer},
        },
    },
    input::{
        SeatHandler,
        pointer::{CursorIcon, CursorImageStatus, MotionEvent},
    },
    reexports::calloop::EventLoop,
    utils::{Logical, Physical, Rectangle, SERIAL_COUNTER, Transform},
};
use wayland_client::{
    Connection, EventQueue, Proxy,
    protocol::{wl_shm, wl_surface},
};
use wayland_server::{Display, protocol::wl_surface::WlSurface};

use super::client::ClientState;
use crate::compositor::{
    App, CalloopData, client::ClientState as ServerClientState,
    frame_callbacks::test_support::app::new_app,
};

pub(super) struct Fixture {
    pub app: App,
    pub renderer: GlesRenderer,
    pub cursor: wl_surface::WlSurface,
    display: Display<App>,
    connection: Connection,
    queue: EventQueue<ClientState>,
    client: ClientState,
    _event_loop: EventLoop<'static, CalloopData>,
}

impl Fixture {
    pub fn new() -> Self {
        let mut display = Display::new().unwrap();
        let mut dh = display.handle();
        let event_loop = EventLoop::try_new().unwrap();
        let mut app = new_app(&dh, event_loop.handle());
        app.software_cursor_visible = true;
        app.cursor_visible = false;
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        let server_client = dh
            .insert_client(server_socket, Arc::new(ServerClientState::default()))
            .unwrap();
        let connection = Connection::from_socket(client_socket).unwrap();
        let mut queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut client = ClientState::default();
        sync(&mut app, &mut display, &connection, &mut queue, &mut client);
        sync(&mut app, &mut display, &connection, &mut queue, &mut client);
        let compositor = client.compositor.as_ref().unwrap();
        let focus = compositor.create_surface(&queue.handle(), ());
        let cursor = compositor.create_surface(&queue.handle(), ());
        sync(&mut app, &mut display, &connection, &mut queue, &mut client);
        let focus: WlSurface = server_client
            .object_from_protocol_id(&dh, focus.id().protocol_id())
            .unwrap();
        // Authenticate real wl_pointer.set_cursor requests. Window placement
        // and input hit-testing are outside this renderer/damage boundary.
        app.pointer.clone().motion(
            &mut app,
            Some((focus, (0., 0.).into())),
            &MotionEvent {
                location: (80., 60.).into(),
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::now(),
            },
        );
        app.pointer.clone().frame(&mut app);
        sync(&mut app, &mut display, &connection, &mut queue, &mut client);
        assert!(client.serial.is_some());
        let egl = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
        let context = EGLContext::new(&egl).unwrap();
        let mut renderer = unsafe { GlesRenderer::new(context).unwrap() };
        renderer
            .with_context(|gl| {
                use smithay::backend::renderer::gles::ffi;
                for (label, name) in [("vendor", ffi::VENDOR), ("renderer", ffi::RENDERER)] {
                    let value = unsafe { gl.GetString(name) };
                    assert!(!value.is_null(), "missing GL {label}");
                    let value = unsafe { std::ffi::CStr::from_ptr(value.cast()) };
                    eprintln!(
                        "cursor damage fixture GL {label}: {}",
                        value.to_string_lossy()
                    );
                }
            })
            .unwrap();
        Self {
            app,
            renderer,
            cursor,
            display,
            connection,
            queue,
            client,
            _event_loop: event_loop,
        }
    }

    pub fn sync(&mut self) {
        sync(
            &mut self.app,
            &mut self.display,
            &self.connection,
            &mut self.queue,
            &mut self.client,
        );
    }

    pub fn image(&mut self, rgba: [u8; 4]) {
        let mut file = tempfile::tempfile().unwrap();
        let bgra = [rgba[2], rgba[1], rgba[0], rgba[3]];
        file.write_all(&bgra.repeat(16 * 12)).unwrap();
        let qh = self.queue.handle();
        let pool =
            self.client
                .shm
                .as_ref()
                .unwrap()
                .create_pool(file.as_fd(), 16 * 12 * 4, &qh, ());
        let buffer = pool.create_buffer(0, 16, 12, 16 * 4, wl_shm::Format::Argb8888, &qh, ());
        self.cursor.attach(Some(&buffer), 0, 0);
        self.cursor.damage_buffer(0, 0, 16, 12);
        self.cursor.commit();
        self.sync();
    }

    pub fn hotspot(&mut self, x: i32, y: i32) {
        self.client.pointer.as_ref().unwrap().set_cursor(
            self.client.serial.unwrap(),
            Some(&self.cursor),
            x,
            y,
        );
        self.sync();
        assert!(self.app.cursor_visible && self.app.cursor_surface.is_some());
    }

    pub fn hide(&mut self) {
        self.client
            .pointer
            .as_ref()
            .unwrap()
            .set_cursor(self.client.serial.unwrap(), None, 0, 0);
        self.sync();
        assert!(!self.app.cursor_visible);
    }

    pub fn named(&mut self, icon: CursorIcon) {
        let seat = self.app._seat.clone();
        self.app.cursor_image(&seat, CursorImageStatus::Named(icon));
    }
}

fn sync(
    app: &mut App,
    display: &mut Display<App>,
    connection: &Connection,
    queue: &mut EventQueue<ClientState>,
    client: &mut ClientState,
) {
    client.synced = false;
    connection.display().sync(&queue.handle(), ());
    connection.flush().unwrap();
    display.dispatch_clients(app).unwrap();
    display.flush_clients().unwrap();
    for _ in 0..16 {
        queue.dispatch_pending(client).unwrap();
        if client.synced {
            assert!(connection.protocol_error().is_none());
            return;
        }
        queue.prepare_read().unwrap().read().unwrap();
    }
    panic!("cursor test Wayland sync did not complete");
}

pub(super) struct Output {
    pub rect: Rectangle<i32, Logical>,
    pub scale: i32,
    buffer: GlesRenderbuffer,
    damage: OutputDamageTracker,
}

pub(super) struct Frame {
    pub damage: Vec<Rectangle<i32, Physical>>,
    pub pixels: Vec<u8>,
    pub software_cursor: bool,
}

impl Output {
    pub fn new(f: &mut Fixture, x: i32, scale: i32) -> Self {
        let rect = Rectangle::new((x, 20).into(), (192, 128).into());
        let size = rect.size.to_physical(scale);
        let buffer = f
            .renderer
            .create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())
            .unwrap();
        Self {
            rect,
            scale,
            buffer,
            damage: OutputDamageTracker::new(size, 1.0, Transform::Normal),
        }
    }

    pub fn render(&mut self, f: &mut Fixture, age: usize) -> Frame {
        let mut target = f.renderer.bind(&mut self.buffer).unwrap();
        let frame = f
            .app
            .render_frame_at(
                &mut f.renderer,
                &mut target,
                &mut self.damage,
                age,
                self.rect,
                self.scale,
            )
            .unwrap();
        let pixels = crate::compositor::capture::read_pixels(
            &mut f.renderer,
            &target,
            self.rect.size.to_physical(self.scale),
            false,
        )
        .unwrap();
        Frame {
            damage: frame.damage.unwrap_or_default(),
            pixels,
            software_cursor: frame.software_cursor,
        }
    }

    #[cfg(feature = "udev")]
    pub fn cursorless_capture(&mut self, f: &mut Fixture) -> Frame {
        let mut target = f.renderer.bind(&mut self.buffer).unwrap();
        let frame = f
            .app
            .render_cursorless_frame_at(
                &mut f.renderer,
                &mut target,
                &self.damage,
                self.rect,
                self.scale,
            )
            .unwrap();
        let pixels = crate::compositor::capture::read_pixels(
            &mut f.renderer,
            &target,
            self.rect.size.to_physical(self.scale),
            false,
        )
        .unwrap();
        Frame {
            damage: frame.damage.unwrap_or_default(),
            pixels,
            software_cursor: frame.software_cursor,
        }
    }

    /// The damage tracker belongs to the output; only its actual target buffer
    /// rotates. A spare Output supplies storage without contributing history.
    #[cfg(feature = "udev")]
    pub fn rotate_buffer(&mut self, spare: &mut Self) {
        assert_eq!(self.rect, spare.rect);
        assert_eq!(self.scale, spare.scale);
        std::mem::swap(&mut self.buffer, &mut spare.buffer);
    }

    /// A fresh full render is an oracle, never the incremental target's history.
    pub fn reference(&self, f: &mut Fixture) -> Frame {
        Self::new(f, self.rect.loc.x, self.scale).render(f, 0)
    }
}
