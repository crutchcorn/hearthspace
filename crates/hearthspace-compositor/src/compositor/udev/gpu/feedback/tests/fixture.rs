use std::{os::fd::AsFd, os::unix::net::UnixStream, sync::Arc};

use smithay::backend::allocator::{Format, Fourcc, Modifier};
use wayland_client::{
    Connection, EventQueue,
    protocol::{wl_registry, wl_surface},
};
use wayland_protocols::{
    wp::linux_dmabuf::zv1::client::{
        zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1, zwp_linux_dmabuf_v1,
    },
    xdg::shell::client::xdg_surface,
};
use wayland_server::Display;

use crate::compositor::App;

use super::{
    client::ClientState,
    server::{ClientDataState, Server},
};

pub(super) fn format(code: Fourcc) -> Format {
    Format {
        code,
        modifier: Modifier::Linear,
    }
}

pub(super) struct Fixture {
    display: Display<App>,
    pub(super) server: Server,
    pub(super) connection: Connection,
    queue: EventQueue<ClientState>,
    pub(super) client: ClientState,
    registry: wl_registry::WlRegistry,
}

impl Fixture {
    pub(super) fn new(device: Option<(u64, Vec<Format>)>, bind_version: u32) -> Self {
        let display = Display::new().unwrap();
        let mut server = Server::new(&display.handle());
        if let Some((device, formats)) = device {
            server.replace(&display.handle(), device, formats).unwrap();
        }
        let (server_socket, client_socket) = UnixStream::pair().unwrap();
        display
            .handle()
            .insert_client(server_socket, Arc::new(ClientDataState::default()))
            .unwrap();
        let connection = Connection::from_socket(client_socket).unwrap();
        let queue = connection.new_event_queue();
        let registry = connection.display().get_registry(&queue.handle(), ());
        let mut fixture = Self {
            display,
            server,
            connection,
            queue,
            registry,
            client: ClientState {
                bind_version,
                ..Default::default()
            },
        };
        fixture.sync();
        fixture.sync();
        fixture
    }

    pub(super) fn sync_result(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // App queues import work during request dispatch. A second barrier also
        // covers the import responses emitted by the first dispatch's drain.
        self.sync_once()?;
        self.sync_once()
    }

    fn sync_once(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.client.synced = false;
        self.connection.display().sync(&self.queue.handle(), ());
        self.connection.flush()?;
        self.display.dispatch_clients(&mut self.server.app)?;
        self.server.reject_imports();
        self.display.flush_clients()?;
        for _ in 0..16 {
            self.queue.dispatch_pending(&mut self.client)?;
            if self.client.synced {
                return Ok(());
            }
            self.queue.prepare_read().unwrap().read()?;
        }
        panic!("Wayland feedback test sync reply was not dispatched");
    }

    pub(super) fn disconnect(self) -> (Display<App>, Server) {
        let Self {
            mut display,
            mut server,
            connection,
            queue,
            client,
            registry,
        } = self;
        drop((connection, queue, client, registry));
        display.dispatch_clients(&mut server.app).unwrap();
        (display, server)
    }

    pub(super) fn sync(&mut self) {
        self.sync_result().unwrap();
    }

    pub(super) fn replace(&mut self, device: u64, formats: Vec<Format>) {
        self.try_replace(device, formats).unwrap();
    }

    pub(super) fn try_replace(&mut self, device: u64, formats: Vec<Format>) -> std::io::Result<()> {
        let result = self.server.replace(&self.display.handle(), device, formats);
        self.sync();
        result
    }

    pub(super) fn dmabuf(&self) -> zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1 {
        self.client.globals.last().unwrap().2.clone()
    }

    pub(super) fn legacy_binding(&mut self, id: usize) -> zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1 {
        let name = self.client.globals.last().unwrap().0;
        let global = self.registry.bind(name, 3, &self.queue.handle(), id);
        self.sync();
        global
    }

    pub(super) fn surface(&self) -> wl_surface::WlSurface {
        self.client
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&self.queue.handle(), ())
    }

    pub(super) fn toplevel(&mut self) -> (wl_surface::WlSurface, xdg_surface::XdgSurface) {
        let surface = self.surface();
        let xdg =
            self.client
                .xdg
                .as_ref()
                .unwrap()
                .get_xdg_surface(&surface, &self.queue.handle(), ());
        xdg.get_toplevel(&self.queue.handle(), ());
        surface.commit();
        self.sync();
        self.sync();
        (surface, xdg)
    }

    pub(super) fn subsurface(&mut self, parent: &wl_surface::WlSurface) -> wl_surface::WlSurface {
        let surface = self.surface();
        self.client.subcompositor.as_ref().unwrap().get_subsurface(
            &surface,
            parent,
            &self.queue.handle(),
            (),
        );
        surface.commit();
        parent.commit();
        self.sync();
        surface
    }

    pub(super) fn popup(&mut self, parent: &xdg_surface::XdgSurface) -> wl_surface::WlSurface {
        let surface = self.surface();
        let qh = self.queue.handle();
        let base = self.client.xdg.as_ref().unwrap();
        let xdg = base.get_xdg_surface(&surface, &qh, ());
        let positioner = base.create_positioner(&qh, ());
        positioner.set_size(10, 10);
        positioner.set_anchor_rect(0, 0, 10, 10);
        xdg.get_popup(Some(parent), &positioner, &qh, ());
        surface.commit();
        self.sync();
        self.sync();
        surface
    }

    pub(super) fn default_feedback(
        &mut self,
        id: usize,
    ) -> zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1 {
        let feedback = self.dmabuf().get_default_feedback(&self.queue.handle(), id);
        self.sync();
        feedback
    }

    pub(super) fn surface_feedback(
        &mut self,
        surface: &wl_surface::WlSurface,
        id: usize,
    ) -> zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1 {
        let feedback = self
            .dmabuf()
            .get_surface_feedback(surface, &self.queue.handle(), id);
        self.sync();
        feedback
    }

    pub(super) fn params(
        &mut self,
        id: usize,
    ) -> zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1 {
        let params = self.dmabuf().create_params(&self.queue.handle(), id);
        // Ensure the server-side params/whitelist exists before a later failover.
        self.sync();
        params
    }

    pub(super) fn submit(
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        format: Format,
    ) {
        let file = tempfile::tempfile().unwrap();
        file.set_len(16).unwrap();
        let modifier: u64 = format.modifier.into();
        params.add(
            file.as_fd(),
            0,
            0,
            8,
            (modifier >> 32) as u32,
            modifier as u32,
        );
        params.create(
            2,
            2,
            format.code as u32,
            zwp_linux_buffer_params_v1::Flags::empty(),
        );
    }

    pub(super) fn assert_feedback(&self, id: usize, count: usize, device: u64, formats: &[Format]) {
        let events = &self.client.feedback[&id].complete;
        assert_eq!(events.len(), count, "feedback object {id}");
        let last = events.last().unwrap();
        assert_eq!(
            last.device,
            (self.client.bind_version < 6).then_some(device)
        );
        assert_eq!(last.table, formats);
        assert_eq!(last.formats, formats);
        assert_eq!(last.targets, [device]);
        assert_eq!(
            last.tranches, 1,
            "feedback must retain a main-device tranche"
        );
        if self.client.bind_version >= 6 {
            assert_eq!(
                last.flags,
                [u32::from(
                    zwp_linux_dmabuf_feedback_v1::TrancheFlags::Sampling
                )]
            );
        }
    }
}
