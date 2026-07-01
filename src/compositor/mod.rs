#[cfg(feature = "winit")]
use std::time::Instant;
use std::{
    os::unix::{fs::MetadataExt, net::UnixListener as CommandListener},
    sync::Arc,
};

use crate::{RunOptions, config::*, geometry::CanvasPoint, shell::app_catalog::AppCatalog};

mod cursor;
mod handlers;
mod idle;
mod input;
mod masonry_titlebar;
mod output;
mod rendering;
mod runtime;
mod shell_integration;
#[cfg(feature = "udev")]
mod udev;
mod viewport;
mod windows;

use idle::{IdleTransition, WindowIdleDaemon};
#[cfg(any(feature = "winit", feature = "udev"))]
pub(in crate::compositor) use input::handle_input_event;
#[cfg(feature = "udev")]
pub(in crate::compositor) use output::{OutputDescriptor, create_output_with_properties};
pub(in crate::compositor) use output::{OutputRecord, OutputSet, create_output};
#[cfg(feature = "udev")]
pub(in crate::compositor) use runtime::create_calloop_data;
pub(in crate::compositor) use runtime::{Backend, CalloopData, HeadlessBackend, run_event_loop};
use shell_integration::{
    accept_command_connections, command_socket_path, remove_stale_socket, spawn_shell,
};
use viewport::ViewportAnimation;
use windows::ResizeEdges;

#[cfg(feature = "udev")]
pub use udev::run_udev;

use calloop::signals::{Signal, Signals};
use cursor::{CursorIcon, SoftwareCursor};
#[cfg(any(feature = "winit", feature = "udev"))]
use smithay::backend::input::TouchSlot;
#[cfg(feature = "winit")]
use smithay::backend::winit::{self, WinitEvent};
#[cfg(feature = "winit")]
use smithay::{backend::renderer::damage::OutputDamageTracker, utils::Transform};
use smithay::{
    backend::{
        allocator::dmabuf::Dmabuf,
        allocator::{Format, Fourcc},
        egl::{EGLContext, EGLDevice, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::{ImportDma, Offscreen, element::Id, gles::GlesRenderer},
    },
    delegate_compositor, delegate_data_device, delegate_dmabuf, delegate_output, delegate_seat,
    delegate_shm, delegate_xdg_decoration, delegate_xdg_shell,
    desktop::PopupManager,
    input::{Seat, SeatState, keyboard::KeyboardHandle, pointer::PointerHandle},
    reexports::{
        calloop::{
            EventLoop, Interest, LoopHandle, Mode as CalloopMode, PostAction, generic::Generic,
        },
        wayland_server::Display,
    },
    utils::{Buffer as BufferCoord, Logical, Physical, Point, Size},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState, ImportNotifier},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::{ToplevelSurface, XdgShellState, decoration::XdgDecorationState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};
use tracing::{debug, error, info};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{
    DisplayHandle,
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::wl_surface::WlSurface,
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
    active_touch_slot: Option<TouchSlot>,
    scroll_zooms_without_super: bool,
    outputs: OutputSet,
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
    software_cursor_visible: bool,
    software_cursor: SoftwareCursor,
}

impl App {
    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn enable_software_cursor(&mut self) {
        self.software_cursor_visible = true;
        self.request_redraw();
    }
}

pub(in crate::compositor) struct AppInit {
    pub(in crate::compositor) display: Display<App>,
    pub(in crate::compositor) event_loop: EventLoop<'static, CalloopData>,
    #[cfg_attr(not(feature = "winit"), allow(dead_code))]
    pub(in crate::compositor) handle: LoopHandle<'static, CalloopData>,
    pub(in crate::compositor) app: App,
}

pub(in crate::compositor) struct DmabufSetup {
    state: DmabufState,
    global: DmabufGlobal,
}

pub(in crate::compositor) fn create_termination_signals()
-> Result<Signals, Box<dyn std::error::Error>> {
    Ok(Signals::new(&[Signal::SIGINT, Signal::SIGTERM])?)
}

#[cfg(feature = "winit")]
pub fn run_winit(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    info!(?options, "initializing winit compositor backend");
    let termination_signals = create_termination_signals()?;
    let display: Display<App> = Display::new()?;
    let dh = display.handle();

    let (mut backend, winit) = winit::init::<GlesRenderer>()?;
    let output_size = backend.window_size();
    debug!(?output_size, "created winit window");
    let primary_output = create_output(&dh, output_size, 1);

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
    let dmabuf = create_dmabuf_global(&dh, dmabuf_formats, render_node_dev)?;
    let AppInit {
        display,
        event_loop,
        handle,
        app: state,
    } = initialize_app(
        display,
        options,
        primary_output,
        dmabuf,
        termination_signals,
    )?;

    // Winit drives input and resize events; it is itself a calloop event source.
    handle.insert_source(winit, |event, _, data| match event {
        WinitEvent::Resized { size, .. } => {
            if size != data.state.output_size() {
                debug!(old_size = ?data.state.output_size(), new_size = ?size, "winit output resized");
                data.state.set_primary_output_size(size);
                data.damage_tracker = OutputDamageTracker::new(size, 1.0, Transform::Flipped180);
                data.full_redraw = 1;
                data.state.configure_shell_bars();
                data.state.reconcile_pointer_after_output_geometry_change();
                data.state.request_redraw();
            }
        }
        WinitEvent::Input(event) => handle_input_event(&mut data.state, event),
        WinitEvent::Redraw => data.state.request_redraw(),
        WinitEvent::CloseRequested => {
            info!("winit close requested; stopping compositor event loop");
            data.running = false;
        }
        WinitEvent::Focus(_) => {}
    })?;

    info!(
        wayland_display = WAYLAND_DISPLAY_NAME,
        "Hearthspace compositor running"
    );
    if state.scroll_zooms_without_super {
        info!("scroll zoom testing mode enabled: vertical scroll zooms without Super");
    }

    let mut data = CalloopData {
        state,
        display,
        backend: Backend::Winit(Box::new(backend)),
        damage_tracker: OutputDamageTracker::new(output_size, 1.0, Transform::Flipped180),
        start_time: Instant::now(),
        running: true,
        exit_at: options.exit_after.map(|duration| Instant::now() + duration),
        full_redraw: 1,
        applied_cursor: CursorIcon::Default,
    };

    run_event_loop(event_loop, &mut data)
}

pub fn run_headless(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    info!(?options, "initializing headless compositor backend");
    let termination_signals = create_termination_signals()?;
    let display: Display<App> = Display::new()?;
    let dh = display.handle();

    let egl_display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay)? };
    let context = EGLContext::new(&egl_display)?;
    let mut renderer = unsafe { GlesRenderer::new(context)? };
    let (width, height) = options
        .headless_output_size
        .unwrap_or((HEADLESS_OUTPUT_WIDTH, HEADLESS_OUTPUT_HEIGHT));
    let output_size = Size::<i32, Physical>::from((width, height));
    let output_scale = options.headless_output_scale.unwrap_or(1);
    debug!(?output_size, output_scale, "created headless output");
    let buffer_size = Size::<i32, BufferCoord>::from((output_size.w, output_size.h));
    let buffer = renderer.create_buffer(Fourcc::Abgr8888, buffer_size)?;
    let primary_output = create_output(&dh, output_size, output_scale);

    let dmabuf_formats = renderer.dmabuf_formats().into_iter().collect::<Vec<_>>();
    let render_node_dev = EGLDevice::device_for_display(renderer.egl_context().display())
        .ok()
        .and_then(|device| device.render_device_path().ok())
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.rdev());
    let dmabuf = create_dmabuf_global(&dh, dmabuf_formats, render_node_dev)?;
    let AppInit {
        display,
        event_loop,
        app: state,
        ..
    } = initialize_app(
        display,
        options,
        primary_output,
        dmabuf,
        termination_signals,
    )?;

    info!(
        wayland_display = WAYLAND_DISPLAY_NAME,
        "headless Hearthspace compositor running"
    );
    if state.scroll_zooms_without_super {
        info!("scroll zoom testing mode enabled: vertical scroll zooms without Super");
    }

    let mut data = runtime::create_headless_calloop_data(
        state,
        display,
        HeadlessBackend { renderer, buffer },
        output_size,
        options.exit_after,
    );

    run_event_loop(event_loop, &mut data)
}

pub(in crate::compositor) fn create_dmabuf_global(
    dh: &DisplayHandle,
    formats: Vec<Format>,
    main_device: Option<u64>,
) -> Result<DmabufSetup, Box<dyn std::error::Error>> {
    let mut state = DmabufState::new();
    debug!(
        format_count = formats.len(),
        main_device, "creating linux-dmabuf global"
    );
    let global = match main_device {
        Some(dev) => {
            let feedback = DmabufFeedbackBuilder::new(dev, formats.iter().copied()).build()?;
            state.create_global_with_default_feedback::<App>(dh, &feedback)
        }
        None => state.create_global::<App>(dh, formats.iter().copied()),
    };
    Ok(DmabufSetup { state, global })
}

pub(in crate::compositor) fn initialize_app(
    mut display: Display<App>,
    options: RunOptions,
    primary_output: OutputRecord,
    dmabuf: DmabufSetup,
    termination_signals: Signals,
) -> Result<AppInit, Box<dyn std::error::Error>> {
    info!(
        start_shell = options.start_shell,
        "initializing compositor app state"
    );
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

    let event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let command_socket_path =
        register_common_event_sources(&mut display, &handle, termination_signals)?;

    let app_catalog = AppCatalog::load();
    debug!(
        app_count = app_catalog.apps().len(),
        "loaded app catalog for compositor"
    );

    let app = App {
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
        active_touch_slot: None,
        scroll_zooms_without_super: options.scroll_zooms_without_super,
        outputs: OutputSet::new(primary_output),
        app_catalog,
        needs_redraw: true,
        dmabuf_state: dmabuf.state,
        _dmabuf_global: dmabuf.global,
        pending_dmabuf_imports: Vec::new(),
        loop_handle: handle.clone(),
        popups: PopupManager::default(),
        background_dot_ids: Vec::new(),
        software_cursor_visible: false,
        software_cursor: cursor::standard_software_cursor(),
    };

    if options.start_shell {
        info!(path = %command_socket_path.display(), "spawning shell client");
        spawn_shell(&command_socket_path);
    }

    Ok(AppInit {
        display,
        event_loop,
        handle,
        app,
    })
}

fn register_common_event_sources(
    display: &mut Display<App>,
    handle: &LoopHandle<'static, CalloopData>,
    termination_signals: Signals,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    handle.insert_source(termination_signals, |event, _, data| {
        info!(signal = ?event.signal(), "received termination signal; stopping compositor event loop");
        data.running = false;
    })?;

    let command_socket_path = command_socket_path();
    remove_stale_socket(&command_socket_path)?;
    let command_listener = CommandListener::bind(&command_socket_path)?;
    command_listener.set_nonblocking(true)?;
    info!(path = %command_socket_path.display(), "listening for shell commands");

    // Accept new Wayland clients (epoll-driven instead of polled).
    let socket_source = ListeningSocketSource::with_name(WAYLAND_DISPLAY_NAME)?;
    handle.insert_source(socket_source, |stream, _, data| {
        match data
            .display
            .handle()
            .insert_client(stream, Arc::new(ClientState::default()))
        {
            Ok(client_id) => debug!(?client_id, "accepted Wayland client"),
            Err(error) => error!(%error, "failed to insert Wayland client"),
        }
        data.state.request_redraw();
    })?;

    // Dispatch client requests when the Wayland display fd becomes readable.
    let display_fd = display.backend().poll_fd().try_clone_to_owned()?;
    handle.insert_source(
        Generic::new(display_fd, Interest::READ, CalloopMode::Level),
        |_, _, data| {
            let CalloopData { state, display, .. } = data;
            if let Err(error) = display.dispatch_clients(state) {
                error!(%error, "failed to dispatch Wayland clients");
            }
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

    Ok(command_socket_path)
}

fn log_idle_transition(transition: IdleTransition) {
    info!(
        window_id = transition.window_id,
        from = ?transition.from,
        to = ?transition.to,
        reason = ?transition.reason,
        "window idle state changed"
    );
}

#[derive(Default)]
struct ClientState {
    compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        debug!(?client_id, "Wayland client initialized");
    }

    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        debug!(?client_id, ?reason, "Wayland client disconnected");
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
