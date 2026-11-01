use std::{
    fs::File,
    os::{fd::AsFd, unix::fs::FileExt, unix::net::UnixStream},
    sync::Arc,
};

use smithay::{
    backend::allocator::{Format, Fourcc, Modifier, dmabuf::Dmabuf},
    reexports::calloop::EventLoop,
};
use wayland_client::{Connection, EventQueue, protocol::wl_buffer};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1;
use wayland_server::Display;

use super::client::ClientState;
use crate::compositor::{
    App, CalloopData, client::ClientState as ServerClientState,
    frame_callbacks::test_support::app::new_app,
};

pub(super) struct Fixture {
    pub(super) app: App,
    pub(super) full_redraw: u8,
    pub(super) events: ClientState,
    pub(super) connection: Connection,
    display: Display<App>,
    queue: EventQueue<ClientState>,
    _event_loop: EventLoop<'static, CalloopData>,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let display = Display::new().unwrap();
        let mut dh = display.handle();
        let event_loop = EventLoop::try_new().unwrap();
        let mut app = new_app(&dh, event_loop.handle());
        // No client is bound yet. Replace the constructor's empty global with
        // real advertised formats so requests reach App's import queue.
        app.dmabuf_state
            .disable_global::<App>(&dh, &app._dmabuf_global);
        app.dmabuf_state
            .destroy_global::<App>(&dh, app._dmabuf_global);
        app._dmabuf_global = app.dmabuf_state.create_global::<App>(
            &dh,
            [Format {
                code: Fourcc::Argb8888,
                modifier: Modifier::Linear,
            }],
        );
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        dh.insert_client(server_socket, Arc::new(ServerClientState::default()))
            .unwrap();
        let connection = Connection::from_socket(client_socket).unwrap();
        let queue = connection.new_event_queue();
        let _registry = connection.display().get_registry(&queue.handle(), ());
        let mut fixture = Self {
            app,
            full_redraw: 0,
            events: ClientState::default(),
            connection,
            display,
            queue,
            _event_loop: event_loop,
        };
        fixture.sync();
        fixture.sync();
        fixture
    }

    /// Dispatch real requests without automatically consuming App's queue.
    pub(super) fn sync_result(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.events.synced = false;
        self.connection.display().sync(&self.queue.handle(), ());
        self.connection.flush()?;
        self.display.dispatch_clients(&mut self.app)?;
        self.display.flush_clients()?;
        for _ in 0..16 {
            self.queue.dispatch_pending(&mut self.events)?;
            if self.events.synced {
                return Ok(());
            }
            self.queue.prepare_read().unwrap().read()?;
        }
        panic!("Wayland import test sync reply was not dispatched");
    }

    pub(super) fn sync(&mut self) {
        self.sync_result().unwrap();
        assert!(self.connection.protocol_error().is_none());
    }

    pub(super) fn submit(&mut self, id: usize, immediate: bool) -> Option<wl_buffer::WlBuffer> {
        let params = self
            .events
            .dmabuf
            .as_ref()
            .unwrap()
            .create_params(&self.queue.handle(), id);
        // This file satisfies the protocol's plane/size checks. GPU validation
        // is the only substituted operation; its marker proves FD retention.
        let file = tempfile::tempfile().unwrap();
        file.set_len(16).unwrap();
        file.write_all_at(&[u8::try_from(id).unwrap()], 0).unwrap();
        params.add(file.as_fd(), 0, 0, 8, 0, 0);
        let buffer = if immediate {
            Some(params.create_immed(
                2,
                2,
                Fourcc::Argb8888 as u32,
                zwp_linux_buffer_params_v1::Flags::empty(),
                &self.queue.handle(),
                (),
            ))
        } else {
            params.create(
                2,
                2,
                Fourcc::Argb8888 as u32,
                zwp_linux_buffer_params_v1::Flags::empty(),
            );
            None
        };
        self.sync();
        buffer
    }

    pub(super) fn assert_deferred(&mut self, expected: &[u8]) {
        self.sync();
        assert_eq!(
            self.app
                .pending_dmabuf_imports
                .iter()
                .map(|(buffer, _)| marker(buffer))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(self.events.created.is_empty());
        assert!(self.events.failed.is_empty());
        assert_eq!(self.full_redraw, 0);
    }

    pub(super) fn use_buffer(&mut self, buffer: &wl_buffer::WlBuffer) {
        let surface = self
            .events
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ());
        surface.attach(Some(buffer), 0, 0);
        // The attach request must still address a live wl_buffer after recovery.
        // No renderer/commit is needed to check the resource's protocol lifetime.
        surface.destroy();
        buffer.destroy();
        self.sync();
    }
}

pub(super) fn marker(buffer: &Dmabuf) -> u8 {
    let file = File::from(
        buffer
            .handles()
            .next()
            .unwrap()
            .try_clone_to_owned()
            .unwrap(),
    );
    let mut marker = [0];
    file.read_exact_at(&mut marker, 0).unwrap();
    marker[0]
}
