use std::os::unix::io::OwnedFd;

use smithay::{
    backend::allocator::dmabuf::Dmabuf,
    backend::renderer::utils::on_commit_buffer_handler,
    desktop::{
        PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy,
        find_popup_root_surface,
    },
    input::{
        Seat, SeatHandler,
        pointer::{CursorImageStatus, Focus},
    },
    reexports::{calloop::Interest, wayland_server::protocol::wl_seat},
    utils::{Serial, Size},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
            SurfaceAttributes, add_blocker, add_pre_commit_hook, with_states,
        },
        dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier, get_dmabuf},
        output::OutputHandler,
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
        },
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            decoration::XdgDecorationHandler,
        },
        shm::{ShmHandler, ShmState},
    },
};
use wayland_protocols::xdg::{
    decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
    shell::server::xdg_toplevel,
};
use wayland_server::{
    Client, Resource,
    protocol::{wl_buffer, wl_surface::WlSurface},
};

use super::{
    App, ClientState, DragState, ManagedWindow, ManagedWindowKind, WindowDecoration,
    idle::ActivityReason,
    windows::{
        ResizeEdges, decoration_for_new_window, position_for_new_window, window_kind_for_toplevel,
    },
};

impl BufferHandler for App {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl XdgShellHandler for App {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let kind = window_kind_for_toplevel(&surface);
        let id = self.next_window_id;
        self.next_window_id += 1;
        self.windows.push(ManagedWindow {
            id,
            surface: surface.clone(),
            position: position_for_new_window(kind, self.next_spawn_position),
            kind,
            decoration: decoration_for_new_window(kind),
            titlebar: None,
            content_bbox_size: Size::default(),
        });
        if kind == ManagedWindowKind::Normal {
            self.idle_daemon.register_window(id);
        }
        self.enter_primary_output(surface.wl_surface());
        self.request_redraw();

        self.configure_toplevel(&surface, kind);
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        // Record the popup's geometry from the client's positioner. The initial
        // configure is sent on the popup's first commit (see `commit`), not here:
        // sending it at role-creation time races the client's first commit so the
        // configured geometry never lands in the surface's current state, leaving
        // the popup mispositioned the first time it opens.
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
        });
        if let Err(err) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            eprintln!("Failed to track popup: {err}");
        }
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        let Some(window_index) = self
            .windows
            .iter()
            .position(|window| window.surface == surface)
        else {
            return;
        };

        if self.windows[window_index].kind != ManagedWindowKind::Normal {
            return;
        }

        let window_index = self.raise_window(window_index);
        let surface = self.windows[window_index].surface.wl_surface().clone();
        self.set_keyboard_focus_to_window(window_index, surface);
        self.drag = Some(DragState {
            window_id: self.windows[window_index].id,
            pointer_start: self.pointer_location,
            window_start: self.windows[window_index].position,
        });
        self.request_redraw();
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let edges = ResizeEdges::from(edges);
        if edges.is_empty() {
            return;
        }
        let Some(window_index) = self
            .windows
            .iter()
            .position(|window| window.surface == surface)
        else {
            return;
        };

        if self.windows[window_index].kind != ManagedWindowKind::Normal {
            return;
        }

        let window_index = self.raise_window(window_index);
        let surface = self.windows[window_index].surface.wl_surface().clone();
        self.set_keyboard_focus_to_window(window_index, surface);
        self.start_resize(window_index, edges);
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        // Set up a popup grab so the menu behaves modally: keyboard and pointer
        // input route to the popup, and clicking elsewhere dismisses it.
        let Some(seat) = Seat::<Self>::from_resource(&seat) else {
            return;
        };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let mut grab = match self.popups.grab_popup(root, kind, &seat, serial) {
            Ok(grab) => grab,
            Err(_) => return,
        };

        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial)
                    || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }

        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.windows.iter().find(|window| window.surface == surface) {
            self.idle_daemon.unregister_window(window.id);
            if self.focused_normal_window_id == Some(window.id) {
                self.focused_normal_window_id = None;
            }
        }
        self.windows.retain(|window| window.surface != surface);
        self.drag = None;
        self.resize = None;
        self.request_redraw();
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        let kind = window_kind_for_toplevel(&surface);
        if let Some(window_index) = self
            .windows
            .iter()
            .position(|window| window.surface == surface)
        {
            let mut window = self.windows.remove(window_index);
            let old_kind = window.kind;
            window.kind = kind;
            window.position = position_for_new_window(kind, window.position);
            if kind.is_shell_chrome() {
                window.decoration = WindowDecoration::ClientSide;
            }
            if old_kind == ManagedWindowKind::Normal && kind != ManagedWindowKind::Normal {
                self.idle_daemon.unregister_window(window.id);
                if self.focused_normal_window_id == Some(window.id) {
                    self.focused_normal_window_id = None;
                }
            } else if old_kind != ManagedWindowKind::Normal && kind == ManagedWindowKind::Normal {
                self.idle_daemon.register_window(window.id);
            }
            let insert_index = match kind {
                ManagedWindowKind::Normal => self.normal_insert_index(),
                ManagedWindowKind::ShellBar | ManagedWindowKind::Launcher => self.windows.len(),
            };
            self.windows.insert(insert_index, window);
            self.configure_toplevel(&surface, kind);
            self.request_redraw();
        }
    }

    fn title_changed(&mut self, _surface: ToplevelSurface) {
        self.request_redraw();
    }
}

impl XdgDecorationHandler for App {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.set_window_decoration(&toplevel, WindowDecoration::ClientSide);
        self.request_redraw();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        let decoration = match mode {
            DecorationMode::ClientSide => WindowDecoration::ClientSide,
            DecorationMode::ServerSide => WindowDecoration::ServerSide,
            _ => WindowDecoration::ServerSide,
        };
        self.set_window_decoration(&toplevel, decoration);
        self.request_redraw();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.set_window_decoration(&toplevel, WindowDecoration::ClientSide);
        self.request_redraw();
    }
}

impl DmabufHandler for App {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        // The renderer lives on the winit backend (not in `App`), so the import
        // is deferred to the event loop where the renderer is reachable.
        self.pending_dmabuf_imports.push((dmabuf, notifier));
        self.request_redraw();
    }
}

impl SelectionHandler for App {
    type SelectionUserData = ();
}

impl DataDeviceHandler for App {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for App {}
impl ServerDndGrabHandler for App {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl CompositorHandler for App {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        // Defer applying a commit until the client's buffer is actually ready.
        //
        // GPU clients attach a dmabuf together with an implicit-sync fence that
        // only signals once their rendering has finished. Compositing before the
        // fence signals risks sampling a half-drawn buffer (tearing/corruption);
        // on real hardware the right behaviour is to wait for the fence on the
        // GPU timeline rather than spin on the CPU.
        //
        // We follow the standard Smithay/anvil approach, which mirrors how
        // Mutter/KWin handle this: attach a blocker on commit that holds the
        // transaction until a calloop source polling the fence fires, so the
        // wait happens asynchronously instead of blocking the event loop.
        add_pre_commit_hook::<Self, _>(surface, |state, _dh, surface| {
            let maybe_dmabuf = with_states(surface, |states| {
                let mut guard = states.cached_state.get::<SurfaceAttributes>();
                match guard.pending().buffer.as_ref() {
                    Some(BufferAssignment::NewBuffer(buffer)) => get_dmabuf(buffer).ok().cloned(),
                    _ => None,
                }
            });
            let Some(dmabuf) = maybe_dmabuf else {
                return;
            };
            // `Err(AlreadyReady)` means the fence is already signalled, so the
            // commit can proceed immediately with no blocker (the common case).
            let Ok((blocker, source)) = dmabuf.generate_blocker(Interest::READ) else {
                return;
            };
            let Some(client) = surface.client() else {
                return;
            };
            let inserted = state.loop_handle.insert_source(source, move |_, _, data| {
                let dh = data.display.handle();
                data.state
                    .client_compositor_state(&client)
                    .blocker_cleared(&mut data.state, &dh);
                Ok(())
            });
            if inserted.is_ok() {
                add_blocker(surface, blocker);
            }
        });
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.popups.commit(surface);
        // Send the popup's initial configure on its first commit. xdg requires
        // the client to commit once (without a buffer) to request a configure;
        // doing it here (rather than in `new_popup`) ensures the configured
        // geometry is applied to the surface's current state.
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface)
            && !popup.is_initial_configure_sent()
            && let Err(err) = popup.send_configure()
        {
            eprintln!("Failed to send initial popup configure: {err}");
        }
        self.refresh_window_content_bbox(surface);
        self.reanchor_resize(surface);
        if let Some(window_id) = self.managed_normal_window_id_for_surface(surface) {
            self.idle_daemon
                .record_activity(window_id, ActivityReason::SurfaceCommit);
        }
        self.request_redraw();
    }
}

impl ShmHandler for App {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl OutputHandler for App {}

impl SeatHandler for App {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut smithay::input::SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}
}
