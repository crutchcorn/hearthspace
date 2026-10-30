//! Real Wayland requests and callback events, with no renderer or KMS device.
//! Native completion tests can call `send_output` from their production dispatch
//! closure; only the external display-completion source needs substitution.
use std::{os::fd::AsFd, os::unix::net::UnixStream, sync::Arc};

use smithay::utils::{Logical, Rectangle};
use wayland_client::{
    Connection, EventQueue,
    protocol::{wl_shm, wl_surface},
};
use wayland_protocols::{
    ext::session_lock::v1::client::{ext_session_lock_surface_v1, ext_session_lock_v1},
    xdg::shell::client::xdg_surface,
};
use wayland_server::{Display, protocol::wl_surface::WlSurface};

use super::{callback_output, dispatch_callbacks};

pub(in crate::compositor) mod app;
mod client;
mod server;
use client::ClientState;
use server::{ClientDataState, Server};

struct Surface {
    client: wl_surface::WlSurface,
    server: WlSurface,
    xdg: Option<xdg_surface::XdgSurface>,
    lock: Option<ext_session_lock_surface_v1::ExtSessionLockSurfaceV1>,
}

struct Window {
    surface: usize,
    bounds: Rectangle<i32, Logical>,
    visible: bool,
    fallback: Option<String>,
}

pub(in crate::compositor) struct Fixture {
    display: Display<Server>,
    server: Server,
    connection: Connection,
    queue: EventQueue<ClientState>,
    client: ClientState,
    surfaces: Vec<Surface>,
    windows: Vec<Window>,
    outputs: Vec<(&'static str, Rectangle<i32, Logical>)>,
    lock: Option<ext_session_lock_v1::ExtSessionLockV1>,
}

impl Fixture {
    pub(in crate::compositor) fn new() -> Self {
        let display = Display::new().unwrap();
        let server = Server::new(&display.handle());
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        display
            .handle()
            .insert_client(server_socket, Arc::new(ClientDataState::default()))
            .unwrap();
        let connection = Connection::from_socket(client_socket).unwrap();
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut fixture = Self {
            display,
            server,
            connection,
            queue,
            client: ClientState::default(),
            surfaces: Vec::new(),
            windows: Vec::new(),
            outputs: vec![
                ("A", Rectangle::new((0, 0).into(), (100, 100).into())),
                ("B", Rectangle::new((100, 0).into(), (100, 100).into())),
            ],
            lock: None,
        };
        fixture.sync();
        fixture.sync();
        fixture
    }

    fn sync(&mut self) {
        self.client.synced = false;
        self.connection
            .display()
            .sync(&self.queue.handle(), None::<usize>);
        self.connection.flush().unwrap();
        self.display.dispatch_clients(&mut self.server).unwrap();
        self.display.flush_clients().unwrap();
        // Both endpoints run here. No threads, sleeps, EGL or native session.
        for _ in 0..16 {
            self.queue.dispatch_pending(&mut self.client).unwrap();
            if self.client.synced {
                return;
            }
            self.queue.prepare_read().unwrap().read().unwrap();
        }
        panic!("Wayland callback test sync reply was not dispatched");
    }

    fn surface(&mut self) -> usize {
        let first = self.server.surfaces.len();
        let client = self
            .client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ());
        self.sync();
        assert_eq!(self.server.surfaces.len(), first + 1);
        let index = self.surfaces.len();
        self.surfaces.push(Surface {
            client,
            server: self.server.surfaces[first].clone(),
            xdg: None,
            lock: None,
        });
        index
    }

    pub(in crate::compositor) fn window(&mut self, output: &str) -> usize {
        let bounds = self
            .outputs
            .iter()
            .find(|(name, _)| *name == output)
            .unwrap()
            .1;
        self.window_at(bounds, true, None)
    }

    pub(super) fn window_at(
        &mut self,
        bounds: Rectangle<i32, Logical>,
        visible: bool,
        fallback: Option<&str>,
    ) -> usize {
        let index = self.surface();
        let surface = &self.surfaces[index].client;
        let xdg =
            self.client
                .xdg
                .as_ref()
                .unwrap()
                .get_xdg_surface(surface, &self.queue.handle(), ());
        xdg.get_toplevel(&self.queue.handle(), ());
        surface.commit();
        self.surfaces[index].xdg = Some(xdg);
        self.windows.push(Window {
            surface: index,
            bounds,
            visible,
            fallback: fallback.map(str::to_owned),
        });
        self.sync();
        self.sync();
        index
    }

    pub(super) fn subsurface(&mut self, parent: usize) -> usize {
        let index = self.surface();
        let surface = &self.surfaces[index].client;
        let role = self.client.subcompositor.as_ref().unwrap().get_subsurface(
            surface,
            &self.surfaces[parent].client,
            &self.queue.handle(),
            (),
        );
        role.set_desync();
        surface.commit();
        self.sync();
        index
    }

    pub(super) fn popup(&mut self, parent: usize) -> usize {
        let index = self.surface();
        let qh = self.queue.handle();
        let base = self.client.xdg.as_ref().unwrap();
        let xdg = base.get_xdg_surface(&self.surfaces[index].client, &qh, ());
        let positioner = base.create_positioner(&qh, ());
        positioner.set_size(10, 10);
        positioner.set_anchor_rect(0, 0, 10, 10);
        xdg.get_popup(self.surfaces[parent].xdg.as_ref(), &positioner, &qh, ());
        self.surfaces[index].client.commit();
        self.surfaces[index].xdg = Some(xdg);
        self.sync();
        self.sync();
        index
    }

    pub(in crate::compositor) fn request_frame(&mut self, surface: usize) {
        self.surfaces[surface]
            .client
            .frame(&self.queue.handle(), Some(surface));
        self.surfaces[surface].client.commit();
        self.sync();
    }

    /// Runs the same owner selection, lock isolation, popup expansion and tree
    /// walker as CalloopData. The caller supplies a verified output completion.
    pub(in crate::compositor) fn send_output(&mut self, output: &str, time: u32) -> usize {
        self.send(Some(output), time)
    }

    pub(super) fn send(&mut self, output: Option<&str>, time: u32) -> usize {
        let windows = self.windows.iter().map(|window| {
            let owner = callback_output(
                &self.outputs,
                Some(window.bounds),
                |_| window.visible,
                window.fallback.as_deref(),
            );
            (self.surfaces[window.surface].server.clone(), owner)
        });
        let count = dispatch_callbacks(
            windows,
            self.server.locked.then_some(self.server.locks.as_slice()),
            output,
            time,
        );
        self.display.flush_clients().unwrap();
        count
    }

    pub(in crate::compositor) fn assert_done(&mut self, expected: &[(usize, u32)]) {
        self.sync();
        let mut actual = std::mem::take(&mut self.client.done);
        let mut expected = expected.to_vec();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected);
    }

    pub(super) fn lock(&mut self) {
        self.lock = Some(
            self.client
                .lock_manager
                .as_ref()
                .unwrap()
                .lock(&self.queue.handle(), ()),
        );
        self.sync();
    }

    pub(super) fn lock_surface(&mut self, output: &str, ready: bool) -> usize {
        let index = self.surface();
        self.surfaces[index].lock = Some(self.lock.as_ref().unwrap().get_lock_surface(
            &self.surfaces[index].client,
            &self.client.outputs[output],
            &self.queue.handle(),
            (),
        ));
        self.sync();
        self.sync();
        if ready {
            self.make_ready(index);
        }
        index
    }

    pub(super) fn make_ready(&mut self, index: usize) {
        let file = tempfile::tempfile().unwrap();
        file.set_len(4).unwrap();
        let qh = self.queue.handle();
        let pool = self
            .client
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), 4, &qh, ());
        let buffer = pool.create_buffer(0, 1, 1, 4, wl_shm::Format::Argb8888, &qh, ());
        self.surfaces[index].client.attach(Some(&buffer), 0, 0);
        self.surfaces[index].client.commit();
        self.sync();
    }

    pub(super) fn destroy_lock_role(&mut self, index: usize) {
        self.surfaces[index].lock.take().unwrap().destroy();
        self.sync();
    }
}
