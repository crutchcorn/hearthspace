use std::{
    os::unix::{fs::MetadataExt, io::OwnedFd, net::UnixListener as CommandListener},
    sync::Arc,
};

use crate::{RunOptions, config::*, geometry::CanvasPoint, shell::app_catalog::AppCatalog};

mod idle;
mod input;
mod masonry_titlebar;
mod rendering;
mod shell_integration;
mod viewport;
mod windows;

use idle::{ActivityReason, IdleTransition, WindowIdleDaemon};
use input::handle_input_event;
use rendering::send_frames_surface_tree;
use shell_integration::{
    accept_command_connections, command_socket_path, remove_stale_socket, spawn_shell,
};
use viewport::ViewportAnimation;
use windows::{
    ResizeEdges, decoration_for_new_window, position_for_new_window, window_kind_for_toplevel,
};

use smithay::reexports::winit::window::CursorIcon;
use smithay::{
    backend::{
        allocator::Fourcc,
        allocator::dmabuf::Dmabuf,
        egl::{EGLContext, EGLDevice, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::{
            Bind, ExportMem, ImportDma, Offscreen,
            damage::OutputDamageTracker,
            element::Id,
            gles::{GlesRenderbuffer, GlesRenderer},
            utils::on_commit_buffer_handler,
        },
        winit::{self, WinitEvent},
    },
    delegate_compositor, delegate_data_device, delegate_dmabuf, delegate_output, delegate_seat,
    delegate_shm, delegate_xdg_decoration, delegate_xdg_shell,
    desktop::{
        PopupKeyboardGrab, PopupKind, PopupManager, PopupPointerGrab, PopupUngrabStrategy,
        find_popup_root_surface,
    },
    input::{
        Seat, SeatHandler, SeatState,
        keyboard::KeyboardHandle,
        pointer::{Focus, PointerHandle},
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        calloop::{
            EventLoop, Interest, LoopHandle, Mode as CalloopMode, PostAction, generic::Generic,
        },
        wayland_server::{Display, protocol::wl_seat},
    },
    utils::{Buffer as BufferCoord, Logical, Physical, Point, Rectangle, Serial, Size, Transform},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
            SurfaceAttributes, add_blocker, add_pre_commit_hook, with_states,
        },
        dmabuf::{
            DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier,
            get_dmabuf,
        },
        output::{OutputHandler, OutputManagerState},
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
        },
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            decoration::{XdgDecorationHandler, XdgDecorationState},
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
    },
};
use wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{
    Client, DisplayHandle, Resource,
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::{wl_buffer, wl_surface::WlSurface},
};

struct ManagedWindow {
    id: u64,
    surface: ToplevelSurface,
    position: CanvasPoint,
    kind: ManagedWindowKind,
    decoration: WindowDecoration,
    /// Masonry-rasterized title-bar image (background, title, close button),
    /// lazily built and cached. See [`masonry_titlebar`].
    titlebar: Option<masonry_titlebar::TitlebarBuffer>,
    /// Bounding-box size of the window's surface tree, cached on commit so the
    /// per-frame rendering and hit-testing paths don't re-walk the tree.
    content_bbox_size: Size<i32, Logical>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedWindowKind {
    Normal,
    ShellBar,
    /// The launcher palette: a transient shell surface the shell opens below the
    /// bar to show app-search results as a dropdown.
    Launcher,
}

impl ManagedWindowKind {
    /// Whether this kind is shell chrome (the bar or the launcher palette),
    /// which is rendered in screen space without server-side decorations rather
    /// than as a normal canvas window.
    fn is_shell_chrome(self) -> bool {
        matches!(self, Self::ShellBar | Self::Launcher)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowDecoration {
    ServerSide,
    ClientSide,
}

struct DragState {
    window_id: u64,
    pointer_start: Point<f64, Logical>,
    window_start: CanvasPoint,
}

struct ResizeState {
    window_id: u64,
    edges: ResizeEdges,
    pointer_start: Point<f64, Logical>,
    initial_position: CanvasPoint,
    initial_content_size: Size<i32, Logical>,
}

enum HitTarget {
    CloseButton {
        window_index: usize,
    },
    TitleBar {
        window_index: usize,
    },
    ResizeBorder {
        window_index: usize,
        edges: ResizeEdges,
    },
    Client {
        window_index: usize,
        surface: WlSurface,
        surface_location: Point<f64, Logical>,
    },
}

struct App {
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    _xdg_decoration_state: XdgDecorationState,
    _output_manager_state: OutputManagerState,
    shm_state: ShmState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    _seat: Seat<Self>,
    pointer: PointerHandle<Self>,
    keyboard: KeyboardHandle<Self>,
    viewport_offset: CanvasPoint,
    viewport_scale: f64,
    viewport_animation: Option<ViewportAnimation>,
    windows: Vec<ManagedWindow>,
    next_window_id: u64,
    idle_daemon: WindowIdleDaemon,
    focused_normal_window_id: Option<u64>,
    drag: Option<DragState>,
    resize: Option<ResizeState>,
    /// Cursor the compositor wants the host winit window to show. Updated from
    /// pointer-motion hit-testing and applied to the backend by the event loop
    /// (which owns the winit window, a sibling of this handler state).
    cursor_icon: CursorIcon,
    next_spawn_position: CanvasPoint,
    spawn_offset: i32,
    pointer_location: Point<f64, Logical>,
    scroll_zooms_without_super: bool,
    output_size: Size<i32, Physical>,
    output: Output,
    app_catalog: AppCatalog,
    needs_redraw: bool,
    dmabuf_state: DmabufState,
    _dmabuf_global: DmabufGlobal,
    pending_dmabuf_imports: Vec<(Dmabuf, ImportNotifier)>,
    loop_handle: LoopHandle<'static, CalloopData>,
    popups: PopupManager,
    /// Stable render-element ids for the background dot grid, grown as the
    /// number of visible dots requires. Reusing ids across frames keeps the
    /// damage tracker from treating every dot as new on each redraw.
    background_dot_ids: Vec<Id>,
}

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
        self.output.enter(surface.wl_surface());
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

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }
}

pub fn run_winit(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let mut display: Display<App> = Display::new()?;
    let dh = display.handle();

    let compositor_state = CompositorState::new::<App>(&dh);
    let xdg_decoration_state = XdgDecorationState::new::<App>(&dh);
    let output_manager_state = OutputManagerState::new_with_xdg_output::<App>(&dh);
    let shm_state = ShmState::new::<App>(&dh, vec![]);
    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "hearthspace");
    let keyboard = seat.add_keyboard(
        Default::default(),
        KEYBOARD_REPEAT_DELAY_MS,
        KEYBOARD_REPEAT_RATE,
    )?;
    let pointer = seat.add_pointer();

    let (mut backend, winit) = winit::init::<GlesRenderer>()?;
    let output_size = backend.window_size();
    let output = create_output(&dh, output_size);

    // Advertise linux-dmabuf so GPU-accelerated clients (e.g. GTK4's GL
    // renderer) can hand us hardware buffers instead of failing EGL setup. The
    // feedback's main device is the render node backing our own GLES renderer,
    // which tells the client's Mesa which GPU to allocate against.
    let dmabuf_formats = backend
        .renderer()
        .dmabuf_formats()
        .into_iter()
        .collect::<Vec<_>>();
    let render_node_dev = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .ok()
        .and_then(|device| device.render_device_path().ok())
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.rdev());
    let mut dmabuf_state = DmabufState::new();
    let dmabuf_global = match render_node_dev {
        Some(dev) => {
            let feedback =
                DmabufFeedbackBuilder::new(dev, dmabuf_formats.iter().copied()).build()?;
            dmabuf_state.create_global_with_default_feedback::<App>(&dh, &feedback)
        }
        None => dmabuf_state.create_global::<App>(&dh, dmabuf_formats.iter().copied()),
    };

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let handle = event_loop.handle();

    let state = App {
        compositor_state,
        xdg_shell_state: XdgShellState::new_with_capabilities::<App>(
            &dh,
            std::iter::empty::<xdg_toplevel::WmCapabilities>(),
        ),
        _xdg_decoration_state: xdg_decoration_state,
        _output_manager_state: output_manager_state,
        shm_state,
        seat_state,
        data_device_state: DataDeviceState::new::<App>(&dh),
        _seat: seat,
        pointer,
        keyboard,
        viewport_offset: CanvasPoint { x: 0, y: 0 },
        viewport_scale: 1.0,
        viewport_animation: None,
        windows: Vec::new(),
        next_window_id: 1,
        idle_daemon: WindowIdleDaemon::new(WINDOW_IDLE_THRESHOLDS),
        focused_normal_window_id: None,
        drag: None,
        resize: None,
        cursor_icon: CursorIcon::Default,
        next_spawn_position: CanvasPoint { x: 80, y: 96 },
        spawn_offset: 0,
        pointer_location: (0.0, 0.0).into(),
        scroll_zooms_without_super: options.scroll_zooms_without_super,
        output_size,
        output,
        app_catalog: AppCatalog::load(),
        needs_redraw: true,
        dmabuf_state,
        _dmabuf_global: dmabuf_global,
        pending_dmabuf_imports: Vec::new(),
        loop_handle: handle.clone(),
        popups: PopupManager::default(),
        background_dot_ids: Vec::new(),
    };

    let command_socket_path = command_socket_path();
    remove_stale_socket(&command_socket_path)?;
    let command_listener = CommandListener::bind(&command_socket_path)?;
    command_listener.set_nonblocking(true)?;

    // Accept new Wayland clients (epoll-driven instead of polled).
    let socket_source = ListeningSocketSource::with_name(WAYLAND_DISPLAY_NAME)?;
    handle.insert_source(socket_source, |stream, _, data| {
        if let Err(error) = data
            .display
            .handle()
            .insert_client(stream, Arc::new(ClientState::default()))
        {
            eprintln!("Failed to insert Wayland client: {error}");
        }
        data.state.request_redraw();
    })?;

    // Dispatch client requests when the Wayland display fd becomes readable.
    let display_fd = display.backend().poll_fd().try_clone_to_owned()?;
    handle.insert_source(
        Generic::new(display_fd, Interest::READ, CalloopMode::Level),
        |_, _, data| {
            let CalloopData { state, display, .. } = data;
            display.dispatch_clients(state)?;
            Ok(PostAction::Continue)
        },
    )?;

    // Accept shell command connections. Each connection is then read
    // incrementally on its own non-blocking source, so a slow or stuck client
    // can never stall the compositor's event loop.
    let command_loop_handle = handle.clone();
    handle.insert_source(
        Generic::new(command_listener, Interest::READ, CalloopMode::Level),
        move |_, listener, _| {
            accept_command_connections(listener, &command_loop_handle)?;
            Ok(PostAction::Continue)
        },
    )?;

    // Winit drives input and resize events; it is itself a calloop event source.
    handle.insert_source(winit, |event, _, data| match event {
        WinitEvent::Resized { size, .. } => {
            if size != data.state.output_size {
                data.state.output_size = size;
                update_output_mode(&data.state.output, size);
                data.damage_tracker = OutputDamageTracker::new(size, 1.0, Transform::Flipped180);
                data.full_redraw = 1;
                data.state.configure_shell_bars();
                data.state.request_redraw();
            }
        }
        WinitEvent::Input(event) => handle_input_event(&mut data.state, event),
        WinitEvent::Redraw => data.state.request_redraw(),
        WinitEvent::CloseRequested => data.running = false,
        WinitEvent::Focus(_) => {}
    })?;

    if options.start_shell {
        spawn_shell(&command_socket_path);
    }

    println!("Hearthspace running on WAYLAND_DISPLAY={WAYLAND_DISPLAY_NAME}");
    if state.scroll_zooms_without_super {
        println!("Scroll zoom testing mode enabled: vertical scroll zooms without Super");
    }

    let mut data = CalloopData {
        state,
        display,
        backend: Backend::Winit(Box::new(backend)),
        damage_tracker: OutputDamageTracker::new(output_size, 1.0, Transform::Flipped180),
        start_time: std::time::Instant::now(),
        running: true,
        full_redraw: 1,
        applied_cursor: CursorIcon::Default,
    };

    data.render()?;

    while data.running {
        // Block until an event arrives; while animating, wake every frame.
        let timeout = data
            .state
            .viewport_animation
            .is_some()
            .then_some(ANIMATION_FRAME_INTERVAL);
        event_loop.dispatch(timeout, &mut data)?;

        if !data.running {
            break;
        }

        data.process_pending_dmabuf_imports();
        data.state.handle_idle_transitions();
        data.state.advance_viewport_animation();
        data.apply_cursor_icon();

        if data.state.needs_redraw {
            data.render()?;
            data.state.needs_redraw = false;
        }

        data.display.flush_clients()?;
        data.state.popups.cleanup();
        data.state.output.cleanup();
    }

    Ok(())
}

pub fn run_headless(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let mut display: Display<App> = Display::new()?;
    let dh = display.handle();

    let compositor_state = CompositorState::new::<App>(&dh);
    let xdg_decoration_state = XdgDecorationState::new::<App>(&dh);
    let output_manager_state = OutputManagerState::new_with_xdg_output::<App>(&dh);
    let shm_state = ShmState::new::<App>(&dh, vec![]);
    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "hearthspace");
    let keyboard = seat.add_keyboard(
        Default::default(),
        KEYBOARD_REPEAT_DELAY_MS,
        KEYBOARD_REPEAT_RATE,
    )?;
    let pointer = seat.add_pointer();

    let egl_display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay)? };
    let context = EGLContext::new(&egl_display)?;
    let mut renderer = unsafe { GlesRenderer::new(context)? };
    let (width, height) = options
        .headless_output_size
        .unwrap_or((HEADLESS_OUTPUT_WIDTH, HEADLESS_OUTPUT_HEIGHT));
    let output_size = Size::<i32, Physical>::from((width, height));
    let buffer_size = Size::<i32, BufferCoord>::from((output_size.w, output_size.h));
    let buffer = renderer.create_buffer(Fourcc::Abgr8888, buffer_size)?;
    let output = create_output(&dh, output_size);

    let dmabuf_formats = renderer.dmabuf_formats().into_iter().collect::<Vec<_>>();
    let render_node_dev = EGLDevice::device_for_display(renderer.egl_context().display())
        .ok()
        .and_then(|device| device.render_device_path().ok())
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.rdev());
    let mut dmabuf_state = DmabufState::new();
    let dmabuf_global = match render_node_dev {
        Some(dev) => {
            let feedback =
                DmabufFeedbackBuilder::new(dev, dmabuf_formats.iter().copied()).build()?;
            dmabuf_state.create_global_with_default_feedback::<App>(&dh, &feedback)
        }
        None => dmabuf_state.create_global::<App>(&dh, dmabuf_formats.iter().copied()),
    };

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let handle = event_loop.handle();

    let state = App {
        compositor_state,
        xdg_shell_state: XdgShellState::new_with_capabilities::<App>(
            &dh,
            std::iter::empty::<xdg_toplevel::WmCapabilities>(),
        ),
        _xdg_decoration_state: xdg_decoration_state,
        _output_manager_state: output_manager_state,
        shm_state,
        seat_state,
        data_device_state: DataDeviceState::new::<App>(&dh),
        _seat: seat,
        pointer,
        keyboard,
        viewport_offset: CanvasPoint { x: 0, y: 0 },
        viewport_scale: 1.0,
        viewport_animation: None,
        windows: Vec::new(),
        next_window_id: 1,
        idle_daemon: WindowIdleDaemon::new(WINDOW_IDLE_THRESHOLDS),
        focused_normal_window_id: None,
        drag: None,
        resize: None,
        cursor_icon: CursorIcon::Default,
        next_spawn_position: CanvasPoint { x: 80, y: 96 },
        spawn_offset: 0,
        pointer_location: (0.0, 0.0).into(),
        scroll_zooms_without_super: options.scroll_zooms_without_super,
        output_size,
        output,
        app_catalog: AppCatalog::load(),
        needs_redraw: true,
        dmabuf_state,
        _dmabuf_global: dmabuf_global,
        pending_dmabuf_imports: Vec::new(),
        loop_handle: handle.clone(),
        popups: PopupManager::default(),
        background_dot_ids: Vec::new(),
    };

    let command_socket_path = command_socket_path();
    remove_stale_socket(&command_socket_path)?;
    let command_listener = CommandListener::bind(&command_socket_path)?;
    command_listener.set_nonblocking(true)?;

    let socket_source = ListeningSocketSource::with_name(WAYLAND_DISPLAY_NAME)?;
    handle.insert_source(socket_source, |stream, _, data| {
        if let Err(error) = data
            .display
            .handle()
            .insert_client(stream, Arc::new(ClientState::default()))
        {
            eprintln!("Failed to insert Wayland client: {error}");
        }
        data.state.request_redraw();
    })?;

    let display_fd = display.backend().poll_fd().try_clone_to_owned()?;
    handle.insert_source(
        Generic::new(display_fd, Interest::READ, CalloopMode::Level),
        |_, _, data| {
            let CalloopData { state, display, .. } = data;
            display.dispatch_clients(state)?;
            Ok(PostAction::Continue)
        },
    )?;

    let command_loop_handle = handle.clone();
    handle.insert_source(
        Generic::new(command_listener, Interest::READ, CalloopMode::Level),
        move |_, listener, _| {
            accept_command_connections(listener, &command_loop_handle)?;
            Ok(PostAction::Continue)
        },
    )?;

    if options.start_shell {
        spawn_shell(&command_socket_path);
    }

    println!("Headless Hearthspace running on WAYLAND_DISPLAY={WAYLAND_DISPLAY_NAME}");
    if state.scroll_zooms_without_super {
        println!("Scroll zoom testing mode enabled: vertical scroll zooms without Super");
    }

    let mut data = CalloopData {
        state,
        display,
        backend: Backend::Headless(Box::new(HeadlessBackend { renderer, buffer })),
        damage_tracker: OutputDamageTracker::new(output_size, 1.0, Transform::Flipped180),
        start_time: std::time::Instant::now(),
        running: true,
        full_redraw: 1,
        applied_cursor: CursorIcon::Default,
    };

    data.render()?;

    while data.running {
        let timeout = data
            .state
            .viewport_animation
            .is_some()
            .then_some(ANIMATION_FRAME_INTERVAL);
        event_loop.dispatch(timeout, &mut data)?;

        if !data.running {
            break;
        }

        data.process_pending_dmabuf_imports();
        data.state.handle_idle_transitions();
        data.state.advance_viewport_animation();
        data.apply_cursor_icon();

        if data.state.needs_redraw {
            data.render()?;
            data.state.needs_redraw = false;
        }

        data.display.flush_clients()?;
        data.state.popups.cleanup();
        data.state.output.cleanup();
    }

    Ok(())
}

enum Backend {
    Winit(Box<smithay::backend::winit::WinitGraphicsBackend<GlesRenderer>>),
    Headless(Box<HeadlessBackend>),
}

struct HeadlessBackend {
    renderer: GlesRenderer,
    buffer: GlesRenderbuffer,
}

struct CalloopData {
    state: App,
    display: Display<App>,
    backend: Backend,
    damage_tracker: OutputDamageTracker,
    start_time: std::time::Instant,
    running: bool,
    // Number of upcoming frames that must be fully redrawn instead of querying
    // the back buffer age. Importing a client dmabuf (or the first frame) can
    // leave the renderer's EGL context surfaceless, which makes `buffer_age`
    // (an `eglQuerySurface` that requires the window surface be current) fail.
    full_redraw: u8,
    // Cursor icon currently applied to the winit window, so the desired cursor
    // (`state.cursor_icon`) is only pushed to the backend when it changes.
    applied_cursor: CursorIcon,
}

impl CalloopData {
    /// Push the compositor's desired cursor to the host winit window, but only
    /// when it differs from the cursor currently shown.
    fn apply_cursor_icon(&mut self) {
        if self.applied_cursor == self.state.cursor_icon {
            return;
        }
        self.applied_cursor = self.state.cursor_icon;
        if let Backend::Winit(backend) = &self.backend {
            backend.window().set_cursor(self.applied_cursor);
        }
    }

    fn process_pending_dmabuf_imports(&mut self) {
        if self.state.pending_dmabuf_imports.is_empty() {
            return;
        }
        let CalloopData { state, backend, .. } = self;
        for (dmabuf, notifier) in state.pending_dmabuf_imports.drain(..) {
            let import = match backend {
                Backend::Winit(backend) => backend.renderer().import_dmabuf(&dmabuf, None),
                Backend::Headless(backend) => backend.renderer.import_dmabuf(&dmabuf, None),
            };
            match import {
                Ok(_texture) => {
                    let _ = notifier.successful::<App>();
                }
                Err(error) => {
                    eprintln!("Failed to import client dmabuf: {error}");
                    notifier.failed();
                }
            }
        }
        // Importing made the renderer's EGL context surfaceless, so skip the
        // next frame's back-buffer-age query and redraw it fully instead.
        self.full_redraw = self.full_redraw.max(1);
    }

    fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let CalloopData {
            state,
            display,
            backend,
            damage_tracker,
            start_time,
            full_redraw,
            ..
        } = self;
        match backend {
            Backend::Winit(backend) => {
                // `buffer_age` is an `eglQuerySurface` that only succeeds while
                // the window surface is the current EGL draw surface. After a
                // dmabuf import (or on the first frame) that is not guaranteed,
                // so those frames are forced to a full redraw (age 0) instead of
                // querying a stale surface.
                let age = if *full_redraw > 0 {
                    *full_redraw = full_redraw.saturating_sub(1);
                    0
                } else {
                    backend.buffer_age().unwrap_or(0)
                };
                let damage = {
                    let (renderer, mut framebuffer) = backend.bind()?;
                    state.render_frame(renderer, &mut framebuffer, damage_tracker, age)?
                };

                if let Some(damage) = damage.as_ref() {
                    backend.submit(Some(damage))?;
                }
            }
            Backend::Headless(backend) => {
                *full_redraw = 0;
                let mut framebuffer = backend.renderer.bind(&mut backend.buffer)?;
                state.render_frame(&mut backend.renderer, &mut framebuffer, damage_tracker, 0)?;
            }
        }

        for window in &state.windows {
            send_frames_surface_tree(
                window.surface.wl_surface(),
                start_time.elapsed().as_millis() as u32,
            );
            // Popups (e.g. client menus) are tracked separately from the window
            // surface tree, so they need their own frame callbacks. Without
            // these the client (e.g. GTK4) throttles and never repaints the
            // popup after its first frame, so keyboard navigation highlights
            // never appear.
            for (popup, _) in PopupManager::popups_for_surface(window.surface.wl_surface()) {
                send_frames_surface_tree(
                    popup.wl_surface(),
                    start_time.elapsed().as_millis() as u32,
                );
            }
        }

        display.flush_clients()?;

        Ok(())
    }

    fn screenshot_png(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.process_pending_dmabuf_imports();

        let CalloopData { state, backend, .. } = self;
        let size = state.output_size;
        let mut screenshot_damage = OutputDamageTracker::new(size, 1.0, Transform::Flipped180);
        let region = Rectangle::from_size(Size::<i32, BufferCoord>::from((size.w, size.h)));
        let pixels = match backend {
            Backend::Winit(backend) => {
                let (renderer, mut framebuffer) = backend.bind()?;
                state.render_frame(renderer, &mut framebuffer, &mut screenshot_damage, 0)?;
                let mapping = renderer.copy_framebuffer(&framebuffer, region, Fourcc::Abgr8888)?;
                renderer.map_texture(&mapping)?.to_vec()
            }
            Backend::Headless(backend) => {
                let mut framebuffer = backend.renderer.bind(&mut backend.buffer)?;
                state.render_frame(
                    &mut backend.renderer,
                    &mut framebuffer,
                    &mut screenshot_damage,
                    0,
                )?;
                let mapping =
                    backend
                        .renderer
                        .copy_framebuffer(&framebuffer, region, Fourcc::Abgr8888)?;
                backend.renderer.map_texture(&mapping)?.to_vec()
            }
        };
        encode_png_rgba(size, &pixels)
    }
}

fn encode_png_rgba(
    size: Size<i32, Physical>,
    bottom_up_rgba: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let width = usize::try_from(size.w)?;
    let height = usize::try_from(size.h)?;
    let stride = width.checked_mul(4).ok_or("screenshot stride overflow")?;
    let expected_len = stride
        .checked_mul(height)
        .ok_or("screenshot buffer length overflow")?;
    if bottom_up_rgba.len() != expected_len {
        return Err(format!(
            "screenshot readback returned {} bytes, expected {expected_len}",
            bottom_up_rgba.len()
        )
        .into());
    }

    let mut top_down_rgba = Vec::with_capacity(expected_len);
    for row in bottom_up_rgba.chunks_exact(stride).rev() {
        top_down_rgba.extend_from_slice(row);
    }

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            &mut png_bytes,
            u32::try_from(width)?,
            u32::try_from(height)?,
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&top_down_rgba)?;
    }
    Ok(png_bytes)
}

fn create_output(dh: &DisplayHandle, size: Size<i32, Physical>) -> Output {
    let output = Output::new(
        "hearthspace-0".into(),
        PhysicalProperties {
            size: (340, 190).into(),
            subpixel: Subpixel::Unknown,
            make: "Hearthspace".into(),
            model: "Nested Canvas".into(),
        },
    );
    output.create_global::<App>(dh);
    update_output_mode(&output, size);
    output
}

fn update_output_mode(output: &Output, size: Size<i32, Physical>) {
    let mode = Mode {
        size,
        refresh: 60_000,
    };
    output.set_preferred(mode);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Integer(1)),
        Some((0, 0).into()),
    );
}

fn log_idle_transition(transition: IdleTransition) {
    println!(
        "Window {} idle transition: {:?} -> {:?} ({:?})",
        transition.window_id, transition.from, transition.to, transition.reason
    );
}

#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {
        println!("initialized");
    }

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {
        println!("disconnected");
    }
}

delegate_xdg_shell!(App);
delegate_compositor!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_data_device!(App);
delegate_output!(App);
delegate_xdg_decoration!(App);
delegate_dmabuf!(App);
