#[cfg(feature = "xwayland")]
use smithay::xwayland::XWaylandClientData;
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
            SurfaceAttributes, TraversalAction, add_blocker, add_pre_commit_hook, with_states,
            with_surface_tree_downward,
        },
        dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier, get_dmabuf},
        fractional_scale::{FractionalScaleHandler, with_fractional_scale},
        output::OutputHandler,
        pointer_constraints::PointerConstraintsHandler,
        selection::{data_device::set_data_device_focus, primary_selection::set_primary_focus},
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            decoration::XdgDecorationHandler,
        },
        shm::{ShmHandler, ShmState},
    },
};
use tracing::{debug, error, info, trace, warn};
use wayland_protocols::xdg::{
    decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
    shell::server::xdg_toplevel,
};
use wayland_server::{
    Client, Resource,
    protocol::{wl_buffer, wl_surface::WlSurface},
};

use super::{
    App, ClientState, ManagedSurface, ManagedWindow, ManagedWindowKind, WindowDecoration,
    focus::KeyboardFocusTarget,
    idle::ActivityReason,
    windows::{
        ResizeEdges, decoration_for_new_window, position_for_new_window,
        shell_output_name_for_toplevel, window_kind_for_toplevel,
    },
};

impl BufferHandler for App {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl XdgShellHandler for App {
    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if !self
            .xdg_window_id(&surface)
            .is_some_and(|id| self.set_window_maximized(id, true).is_ok())
            && surface.is_initial_configure_sent()
        {
            surface.send_configure();
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if !self
            .xdg_window_id(&surface)
            .is_some_and(|id| self.set_window_maximized(id, false).is_ok())
            && surface.is_initial_configure_sent()
        {
            surface.send_configure();
        }
    }

    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        output: Option<wayland_server::protocol::wl_output::WlOutput>,
    ) {
        let output = output
            .as_ref()
            .and_then(smithay::output::Output::from_resource)
            .map(|output| output.name());
        if !self
            .xdg_window_id(&surface)
            .is_some_and(|id| self.set_window_fullscreen(id, true, output).is_ok())
            && surface.is_initial_configure_sent()
        {
            surface.send_configure();
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if !self
            .xdg_window_id(&surface)
            .is_some_and(|id| self.set_window_fullscreen(id, false, None).is_ok())
            && surface.is_initial_configure_sent()
        {
            surface.send_configure();
        }
    }

    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let kind = window_kind_for_toplevel(&surface);
        let shell_output_name = shell_output_name_for_toplevel(&surface);
        let spawn_output_name = (kind == ManagedWindowKind::Normal)
            .then(|| self.next_spawn_output_name.take())
            .flatten();
        let id = self.next_window_id;
        self.next_window_id += 1;
        info!(
            window_id = id,
            ?kind,
            ?shell_output_name,
            "new xdg toplevel"
        );
        self.windows.push(ManagedWindow {
            id,
            surface: ManagedSurface::Xdg(surface.clone()),
            position: position_for_new_window(kind, self.viewport_offset),
            kind,
            shell_output_name: shell_output_name.clone(),
            decoration: decoration_for_new_window(kind),
            titlebar: None,
            content_bbox_size: Size::default(),
            mapped: false,
            initial_placement_done: false,
            initial_output_name: spawn_output_name.clone(),
            presentation: Default::default(),
            advertised_fractional_scale: 0.0,
        });
        if kind == ManagedWindowKind::Normal {
            self.idle_daemon.register_window(id);
        }
        if kind.is_shell_chrome() {
            self.enter_shell_output(surface.wl_surface(), shell_output_name.as_deref());
        } else {
            self.enter_output(surface.wl_surface(), spawn_output_name.as_deref());
        }
        self.request_redraw();

        self.configure_toplevel(&surface, kind, shell_output_name.as_deref());
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        debug!(geometry = ?positioner.get_geometry(), "new xdg popup");
        // Record the popup's geometry from the client's positioner. The initial
        // configure is sent on the popup's first commit (see `commit`), not here:
        // sending it at role-creation time races the client's first commit so the
        // configured geometry never lands in the surface's current state, leaving
        // the popup mispositioned the first time it opens.
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        if let Err(err) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            warn!(%err, "failed to track popup");
        }
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        if self.session_locked() {
            return;
        }
        let Some(window_index) = self
            .windows
            .iter()
            .position(|window| window.surface.same_xdg(&surface))
        else {
            return;
        };

        if self.windows[window_index].kind != ManagedWindowKind::Normal
            || self.window_uses_output_coordinates(window_index)
            || !self.valid_window_pointer_request(window_index, &seat, serial)
        {
            return;
        }

        let window_index = self.raise_window(window_index);
        if let Some(surface) = self.windows[window_index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(window_index, surface);
        }
        debug!(
            window_id = self.windows[window_index].id,
            "client requested window move"
        );
        self.start_client_window_move(window_index);
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        if self.session_locked() {
            return;
        }
        let edges = ResizeEdges::from(edges);
        if edges.is_empty() {
            return;
        }
        let Some(window_index) = self
            .windows
            .iter()
            .position(|window| window.surface.same_xdg(&surface))
        else {
            return;
        };

        if self.windows[window_index].kind != ManagedWindowKind::Normal
            || self.window_uses_output_coordinates(window_index)
            || !self.valid_window_pointer_request(window_index, &seat, serial)
        {
            return;
        }

        let window_index = self.raise_window(window_index);
        if let Some(surface) = self.windows[window_index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(window_index, surface);
        }
        debug!(
            window_id = self.windows[window_index].id,
            ?edges,
            "client requested window resize"
        );
        self.start_resize(window_index, edges);
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        if self.session_locked() || self.overview_active() || self.window_switcher_active() {
            return;
        }
        // Set up a popup grab so the menu behaves modally: keyboard and pointer
        // input route to the popup, and clicking elsewhere dismisses it.
        let Some(seat) = Seat::<Self>::from_resource(&seat) else {
            return;
        };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let mut grab = match self.popups.grab_popup(root.into(), kind, &seat, serial) {
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
            self.interrupt_pinch_zoom();
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
            self.interrupt_pinch_zoom();
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
        self.popup_grab = Some(grab);
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
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn parent_changed(&mut self, surface: ToplevelSurface) {
        if let Some(index) = self
            .windows
            .iter()
            .position(|window| window.surface.same_xdg(&surface))
            && self.windows[index].mapped
        {
            self.position_transient(index);
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let restore_panel_focus = !self.pending_search_reserves_focus()
            && self
                .active_shell_panel_index()
                .is_some_and(|index| self.windows[index].surface.same_xdg(&surface));
        let restore_focus = self.windows.iter().any(|window| {
            window.surface.same_xdg(&surface) && self.focused_normal_window_id == Some(window.id)
        });
        self.clear_keyboard_focus_for_surface(surface.wl_surface());
        if let Some(window) = self
            .windows
            .iter()
            .find(|window| window.surface.same_xdg(&surface))
        {
            info!(window_id = window.id, kind = ?window.kind, "xdg toplevel destroyed");
            self.idle_daemon.unregister_window(window.id);
            if self.focused_normal_window_id == Some(window.id) {
                self.focused_normal_window_id = None;
            }
        }
        self.windows
            .retain(|window| !window.surface.same_xdg(&surface));
        self.drag = None;
        self.resize = None;
        if restore_panel_focus {
            self.restore_focus_after_shell_panel();
        } else if restore_focus {
            self.restore_window_focus();
        }
        self.request_redraw();
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        let kind = window_kind_for_toplevel(&surface);
        let shell_output_name = shell_output_name_for_toplevel(&surface);
        if let Some(window_index) = self
            .windows
            .iter()
            .position(|window| window.surface.same_xdg(&surface))
        {
            let mut window = self.windows.remove(window_index);
            let old_kind = window.kind;
            let old_shell_output_name = window.shell_output_name.clone();
            window.kind = kind;
            window.shell_output_name = shell_output_name.clone();
            info!(
                window_id = window.id,
                from = ?old_kind,
                to = ?kind,
                from_output = ?old_shell_output_name,
                to_output = ?shell_output_name,
                "toplevel app id changed window kind"
            );
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
                ManagedWindowKind::ShellBar
                | ManagedWindowKind::Launcher
                | ManagedWindowKind::ShellWatcher => self.windows.len(),
            };
            let wl_surface = window.surface.wl_surface();
            self.windows.insert(insert_index, window);
            if kind.is_shell_chrome() {
                if let Some(surface) = &wl_surface {
                    self.enter_shell_output(surface, shell_output_name.as_deref());
                }
            } else if let Some(surface) = &wl_surface {
                self.enter_output(surface, None);
            }
            self.configure_toplevel(&surface, kind, shell_output_name.as_deref());
            self.request_redraw();
        }
    }

    fn title_changed(&mut self, _surface: ToplevelSurface) {
        self.request_redraw();
    }
}

impl XdgDecorationHandler for App {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        debug!("new xdg decoration object");
        self.set_window_decoration(&toplevel, WindowDecoration::ClientSide);
        self.request_redraw();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        debug!(?mode, "client requested decoration mode");
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
        trace!(
            pending = self.pending_dmabuf_imports.len(),
            "queued client dmabuf import"
        );
        self.request_redraw();
    }
}

impl CompositorHandler for App {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        if let Some(state) = client.get_data::<ClientState>() {
            return &state.compositor_state;
        }
        #[cfg(feature = "xwayland")]
        if let Some(state) = client.get_data::<XWaylandClientData>() {
            return &state.compositor_state;
        }
        panic!("unknown compositor client data type")
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
                trace!("installed dmabuf commit blocker");
                add_blocker(surface, blocker);
            } else if let Err(error) = inserted {
                error!(%error, "failed to install dmabuf commit blocker source");
            }
        });
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.configure_initial_toplevel(surface);
        self.popups.commit(surface);
        // Send the popup's initial configure on its first commit. xdg requires
        // the client to commit once (without a buffer) to request a configure;
        // doing it here (rather than in `new_popup`) ensures the configured
        // geometry is applied to the surface's current state.
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface)
            && !popup.is_initial_configure_sent()
            && let Err(err) = popup.send_configure()
        {
            warn!(%err, "failed to send initial popup configure");
        }
        self.refresh_window_content_bbox(surface);
        self.configure_shell_palette_size(surface);
        self.lock_surface_committed(surface);
        self.finish_presentation_restore(surface);
        self.refresh_window_mapping(surface);
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

// Smithay uses this hook for pointer-target cleanup even when the optional
// pointer-constraints protocol is not advertised by the compositor.
impl PointerConstraintsHandler for App {}

impl SeatHandler for App {
    type KeyboardFocus = KeyboardFocusTarget;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut smithay::input::SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&KeyboardFocusTarget>) {
        self.sync_window_activation(focused.map(KeyboardFocusTarget::surface));
        set_data_device_focus(
            &self.display_handle,
            seat,
            focused.and_then(|focus| focus.surface().client()),
        );
        set_primary_focus(
            &self.display_handle,
            seat,
            focused.and_then(|focus| focus.surface().client()),
        );
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        match image {
            CursorImageStatus::Hidden => {
                self.cursor_visible = false;
                self.cursor_surface = None;
            }
            CursorImageStatus::Named(icon) => {
                self.cursor_visible = true;
                self.cursor_surface = None;
                self.cursor_icon = crate::compositor::cursor::cursor_icon_from_smithay(icon);
            }
            CursorImageStatus::Surface(surface) => {
                // A focused client wants to draw its own cursor. Present the
                // client surface (with its committed hotspot) as a software
                // cursor; the backend cursor policy falls back to software
                // presentation whenever a client surface is active.
                self.cursor_visible = true;
                self.cursor_surface = Some(surface);
            }
        }
        // The cursor changed, so a frame is needed to update software cursor
        // presentation (and to hand the new shape to the backend cursor policy).
        self.request_redraw();
    }
}

impl FractionalScaleHandler for App {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        // Advertise the preferred fractional scale so HiDPI toolkits (GTK,
        // Firefox) render their buffers at the monitor scale and declare their
        // logical size through `wp_viewport`, instead of rendering an oversized
        // integer-scaled buffer that the compositor would then scale again.
        let scale = self.preferred_fractional_scale(&surface);
        with_states(&surface, |states| {
            with_fractional_scale(states, |fractional_scale| {
                fractional_scale.set_preferred_scale(scale);
            });
        });
    }
}

impl App {
    /// The fractional scale to advertise for `surface`: the integer scale of the
    /// output its window sits on, or the primary output's scale when the surface
    /// is not (yet) a managed window.
    pub(super) fn preferred_fractional_scale(&self, surface: &WlSurface) -> f64 {
        self.window_index_for_surface(surface)
            .map(|index| self.window_fractional_scale(index))
            .unwrap_or_else(|| f64::from(self.primary_output_scale().max(1)))
    }

    /// The scale of the output the window at `index` currently sits on.
    fn window_fractional_scale(&self, index: usize) -> f64 {
        let screen = self.window_surface_screen_origin(index);
        f64::from(self.output_scale_at(screen).max(1))
    }

    /// Re-advertise the preferred fractional scale for every managed window whose
    /// output scale changed since the last frame. Runs each frame so a window
    /// dragged onto a differently-scaled monitor re-renders at the new scale, and
    /// is also triggered on output-layout changes. The surface tree is only
    /// walked when the scale actually changes, so the steady state is cheap.
    pub(super) fn refresh_fractional_scales(&mut self) {
        for index in 0..self.windows.len() {
            let scale = self.window_fractional_scale(index);
            if (self.windows[index].advertised_fractional_scale - scale).abs() < f64::EPSILON {
                continue;
            }
            self.windows[index].advertised_fractional_scale = scale;
            let Some(root) = self.windows[index].surface.wl_surface() else {
                continue;
            };
            with_surface_tree_downward(
                &root,
                (),
                |_, _, _| TraversalAction::DoChildren(()),
                |_, states, _| {
                    with_fractional_scale(states, |fractional_scale| {
                        fractional_scale.set_preferred_scale(scale);
                    });
                },
                |_, _, _| true,
            );
        }
    }
}
