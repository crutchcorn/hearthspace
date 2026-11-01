//! Real App protocol commits and surfaceless EGL, with both Wayland endpoints
//! dispatched synchronously. No session, services, native seat or background loop.
use std::{
    fs::File,
    os::{
        fd::AsFd,
        unix::{fs::FileExt, net::UnixStream},
    },
    sync::Arc,
};

use smithay::{
    backend::{
        egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::gles::{GlesRenderer, ffi},
    },
    input::pointer::MotionEvent,
    reexports::calloop::EventLoop,
    utils::SERIAL_COUNTER,
};
use wayland_client::{
    Connection, EventQueue, Proxy, QueueHandle,
    protocol::{wl_buffer, wl_shm, wl_subsurface, wl_surface},
};
use wayland_protocols::wp::viewporter::client::wp_viewport;
use wayland_server::{Client, Display, protocol::wl_surface::WlSurface};

use super::client::ClientState;
use crate::compositor::{
    App, CalloopData, client::ClientState as ServerClientState,
    frame_callbacks::test_support::app::new_app, output::OutputRenderView,
};

pub(super) struct ShmImage {
    pub(super) buffer: wl_buffer::WlBuffer,
    _file: File,
}

pub(super) struct Fixture {
    pub(super) app: App,
    pub(super) renderer: GlesRenderer,
    client: ClientState,
    display: Display<App>,
    server_client: Client,
    connection: Connection,
    queue: EventQueue<ClientState>,
    _focus: wl_surface::WlSurface,
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
        let egl = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
        let context = EGLContext::new(&egl).unwrap();
        let mut renderer = unsafe { GlesRenderer::new(context).unwrap() };
        renderer
            .with_context(|gl| {
                for (name, token) in [
                    ("vendor", ffi::VENDOR),
                    ("renderer", ffi::RENDERER),
                    ("version", ffi::VERSION),
                ] {
                    // SAFETY: this renderer's context is current. GetString
                    // returns context-owned, NUL-terminated text for these tokens.
                    let value = unsafe { gl.GetString(token) };
                    assert!(!value.is_null(), "missing GL {name}");
                    let value = unsafe { std::ffi::CStr::from_ptr(value.cast()) };
                    eprintln!("cursor snapshot GL {name}: {}", value.to_string_lossy());
                }
            })
            .unwrap();
        // Construct after binding globals, since the focus surface is a real
        // protocol object and cursor requests require its pointer-enter serial.
        let mut client = ClientState::default();
        let mut display = display;
        let mut app = app;
        let mut queue = queue;
        for _ in 0..2 {
            sync(&connection, &mut queue, &mut client, &mut display, &mut app);
        }
        let focus = client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&queue.handle(), ());
        sync(&connection, &mut queue, &mut client, &mut display, &mut app);
        let server_focus: WlSurface = server_client
            .object_from_protocol_id(&handle, focus.id().protocol_id())
            .unwrap();
        app.pointer.clone().motion(
            &mut app,
            Some((server_focus, (0., 0.).into())),
            &MotionEvent {
                location: (1., 1.).into(),
                serial: SERIAL_COUNTER.next_serial(),
                time: smithay::backend::input::InputTime::from_millis(1),
            },
        );
        app.pointer.clone().frame(&mut app);
        sync(&connection, &mut queue, &mut client, &mut display, &mut app);
        assert!(client.enter_serial.is_some());
        Self {
            app,
            renderer,
            client,
            display,
            server_client,
            connection,
            queue,
            _focus: focus,
            _event_loop: event_loop,
        }
    }

    pub(super) fn queue(&self) -> QueueHandle<ClientState> {
        self.queue.handle()
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

    pub(super) fn server_surface(&self, surface: &wl_surface::WlSurface) -> WlSurface {
        self.server_client
            .object_from_protocol_id(&self.display.handle(), surface.id().protocol_id())
            .unwrap()
    }

    pub(super) fn cursor(&mut self, surface: Option<&wl_surface::WlSurface>, hotspot: (i32, i32)) {
        self.client.pointer.as_ref().unwrap().set_cursor(
            self.client.enter_serial.unwrap(),
            surface,
            hotspot.0,
            hotspot.1,
        );
        self.sync();
        assert_eq!(self.app.cursor_visible, surface.is_some());
        if let Some(surface) = surface {
            assert_eq!(self.app.cursor_surface, Some(self.server_surface(surface)));
        }
    }

    pub(super) fn viewport(&self, surface: &wl_surface::WlSurface) -> wp_viewport::WpViewport {
        self.client
            .viewporter
            .as_ref()
            .unwrap()
            .get_viewport(surface, &self.queue.handle(), ())
    }

    pub(super) fn subsurface(
        &self,
        child: &wl_surface::WlSurface,
        parent: &wl_surface::WlSurface,
    ) -> wl_subsurface::WlSubsurface {
        self.client.subcompositor.as_ref().unwrap().get_subsurface(
            child,
            parent,
            &self.queue.handle(),
            (),
        )
    }

    pub(super) fn image(
        &self,
        size: (i32, i32),
        argb: &[u32],
        padding: (i32, i32),
        format: wl_shm::Format,
    ) -> ShmImage {
        let (offset, row_padding) = padding;
        let stride = size.0 * 4 + row_padding;
        let length = offset + stride * size.1;
        assert_eq!(argb.len(), (size.0 * size.1) as usize);
        let file = tempfile::tempfile().unwrap();
        file.set_len(length as u64).unwrap();
        for y in 0..size.1 {
            for x in 0..size.0 {
                file.write_all_at(
                    &argb[(y * size.0 + x) as usize].to_ne_bytes(),
                    (offset + y * stride + x * 4) as u64,
                )
                .unwrap();
            }
        }
        let pool = self.client.shm.as_ref().unwrap().create_pool(
            file.as_fd(),
            length,
            &self.queue.handle(),
            (),
        );
        let buffer = pool.create_buffer(
            offset,
            size.0,
            size.1,
            stride,
            format,
            &self.queue.handle(),
            (),
        );
        pool.destroy();
        ShmImage {
            buffer,
            _file: file,
        }
    }

    pub(super) fn attach(&mut self, surface: &wl_surface::WlSurface, image: &ShmImage) {
        surface.attach(Some(&image.buffer), 0, 0);
        surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
        surface.commit();
        self.sync();
    }

    pub(super) fn unrecognized_image(&mut self) -> ShmImage {
        use wayland_server::{Resource, protocol::wl_buffer::WlBuffer};

        let image = self.image((1, 1), &[0xffff_0000], (0, 0), wl_shm::Format::Argb8888);
        self.sync();
        let buffer: WlBuffer = self
            .server_client
            .object_from_protocol_id(&self.display.handle(), image.buffer.id().protocol_id())
            .unwrap();
        // Fault injection is limited to the storage recognition boundary. The
        // subsequent attach/commit, renderer state and recovery are real App
        // protocol handling; no pending/current surface state is fabricated.
        self.display
            .handle()
            .backend_handle()
            .set_object_data::<App>(
                buffer.id(),
                Arc::new(super::recognition::UnrecognizedBuffer),
            )
            .unwrap();
        let unknown = WlBuffer::from_id(&self.display.handle(), buffer.id()).unwrap();
        assert!(smithay::backend::renderer::buffer_dimensions(&unknown).is_none());
        image
    }

    pub(super) fn sync(&mut self) {
        sync(
            &self.connection,
            &mut self.queue,
            &mut self.client,
            &mut self.display,
            &mut self.app,
        );
    }

    pub(super) fn clear_blocker(&mut self, blocker: &smithay::wayland::compositor::Barrier) {
        blocker.signal();
        self.server_client
            .get_data::<ServerClientState>()
            .unwrap()
            .compositor_state
            .blocker_cleared(&mut self.app, &self.display.handle());
        self.sync();
    }
}

fn sync(
    connection: &Connection,
    queue: &mut EventQueue<ClientState>,
    client: &mut ClientState,
    display: &mut Display<App>,
    app: &mut App,
) {
    client.synced = false;
    connection.display().sync(&queue.handle(), true);
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
    panic!("cursor fixture Wayland sync reply was not dispatched");
}

pub(super) fn view(location: (i32, i32), scale: i32) -> OutputRenderView {
    OutputRenderView {
        name: "cursor-test".into(),
        location: location.into(),
        size: (800, 600).into(),
        scale,
    }
}
