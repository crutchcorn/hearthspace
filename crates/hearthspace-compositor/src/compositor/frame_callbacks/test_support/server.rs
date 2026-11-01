use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    desktop::{PopupKind, PopupManager},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    utils::Serial,
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
        output::OutputHandler,
        session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker},
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        },
        shm::{ShmHandler, ShmState},
    },
};
use wayland_server::{
    Client, DisplayHandle,
    backend::ClientData,
    protocol::{wl_buffer::WlBuffer, wl_output::WlOutput, wl_seat::WlSeat, wl_surface::WlSurface},
};

pub(super) struct Server {
    compositor: CompositorState,
    xdg: XdgShellState,
    shm: ShmState,
    lock: SessionLockManagerState,
    popups: PopupManager,
    pub(super) surfaces: Vec<WlSurface>,
    pub(super) locks: Vec<(String, LockSurface)>,
    pub(super) locked: bool,
}

impl Server {
    pub(super) fn toplevel(&self, surface: &WlSurface) -> ToplevelSurface {
        self.xdg
            .toplevel_surfaces()
            .iter()
            .find(|toplevel| toplevel.wl_surface() == surface)
            .unwrap()
            .clone()
    }

    pub(super) fn new(display: &DisplayHandle) -> Self {
        for name in ["A", "B"] {
            let output = Output::new(
                name.into(),
                PhysicalProperties {
                    size: (100, 100).into(),
                    subpixel: Subpixel::Unknown,
                    make: "Callback test".into(),
                    model: name.into(),
                    serial_number: name.into(),
                },
            );
            output.change_current_state(
                Some(Mode {
                    size: (100, 100).into(),
                    refresh: 60_000,
                }),
                None,
                None,
                None,
            );
            output.create_global::<Self>(display);
        }
        Self {
            compositor: CompositorState::new::<Self>(display),
            xdg: XdgShellState::new::<Self>(display),
            shm: ShmState::new::<Self>(display, []),
            lock: SessionLockManagerState::new::<Self, _>(display, |_| true),
            popups: PopupManager::default(),
            surfaces: Vec::new(),
            locks: Vec::new(),
            locked: false,
        }
    }
}

#[derive(Default)]
pub(super) struct ClientDataState(CompositorClientState);
impl ClientData for ClientDataState {}

impl CompositorHandler for Server {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientDataState>().unwrap().0
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        self.surfaces.push(surface.clone());
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.popups.commit(surface);
    }
}

impl XdgShellHandler for Server {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        surface.send_configure();
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_configure().unwrap();
        self.popups.track_popup(PopupKind::Xdg(surface)).unwrap();
    }

    fn grab(&mut self, _: PopupSurface, _: WlSeat, _: Serial) {}

    fn reposition_request(&mut self, surface: PopupSurface, _: PositionerState, token: u32) {
        surface.send_repositioned(token);
    }
}

impl BufferHandler for Server {
    fn buffer_destroyed(&mut self, _: &WlBuffer) {}
}

impl ShmHandler for Server {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}

impl OutputHandler for Server {}

impl SessionLockHandler for Server {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.lock
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        self.locked = true;
        confirmation.lock();
    }

    fn unlock(&mut self) {
        self.locked = false;
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        surface.with_pending_state(|state| state.size = Some((1, 1).into()));
        surface.send_configure();
        self.locks
            .push((Output::from_resource(&output).unwrap().name(), surface));
    }
}

smithay::delegate_dispatch2!(Server);
