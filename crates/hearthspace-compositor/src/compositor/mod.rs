#[cfg(feature = "winit")]
use std::time::Instant;
use std::{
    collections::HashSet,
    os::unix::{fs::MetadataExt, net::UnixListener as CommandListener},
    sync::Arc,
};

use crate::{RunOptions, config::*, geometry::CanvasPoint};
use hearthspace_desktop::app_catalog::AppCatalog;

mod appearance;
mod canvas_peek;
mod canvas_preview;
mod canvas_search;
mod capture;
mod client;
mod clipboard;
mod cursor;
mod desktop;
mod focus;
mod focused_shell_peek;
mod gtk_shell;
mod handlers;
mod idle;
mod input;
mod layout;
mod layout_editor;
mod masonry_titlebar;
mod navigation;
mod output;
mod presentation;
mod rendering;
mod runtime;
mod selection;
mod session_lock;
mod settings_watch;
mod shell_integration;
mod switcher;
#[cfg(feature = "udev")]
mod udev;
mod viewport;
mod window_frame;
mod window_group;
mod windows;
#[cfg(feature = "xwayland")]
mod xwayland;

use idle::{IdleTransition, WindowIdleDaemon};
#[cfg(feature = "udev")]
pub(in crate::compositor) use input::handle_input_event;
#[cfg(feature = "udev")]
pub(in crate::compositor) use output::{OutputDescriptor, create_output_with_properties};
pub(in crate::compositor) use output::{OutputLayout, OutputRecord, OutputSet, create_output};
#[cfg(any(feature = "winit", feature = "udev"))]
pub(in crate::compositor) use runtime::Backend;
#[cfg(feature = "udev")]
pub(in crate::compositor) use runtime::create_calloop_data;
pub(in crate::compositor) use runtime::{CalloopData, HeadlessBackend, run_event_loop};
use shell_integration::{
    accept_command_connections, command_socket_path, remove_stale_socket, spawn_shell,
};
use viewport::ViewportAnimation;
use windows::ResizeEdges;

#[cfg(feature = "udev")]
pub use udev::run_udev;

use calloop::signals::{Signal, Signals};
use client::ClientState;
use cursor::{CursorIcon, SoftwareCursor};
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
    desktop::PopupManager,
    input::{
        Seat, SeatState, keyboard::KeyboardHandle, pointer::PointerHandle, touch::TouchHandle,
    },
    reexports::{
        calloop::{
            EventLoop, Interest, LoopHandle, Mode as CalloopMode, PostAction, generic::Generic,
        },
        wayland_server::Display,
    },
    utils::{Buffer as BufferCoord, Logical, Physical, Point, Size},
    wayland::{
        compositor::CompositorState,
        dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState, ImportNotifier},
        fractional_scale::FractionalScaleManagerState,
        output::OutputManagerState,
        selection::{data_device::DataDeviceState, primary_selection::PrimarySelectionState},
        shell::xdg::{ToplevelSurface, XdgShellState, decoration::XdgDecorationState},
        shm::ShmState,
        socket::ListeningSocketSource,
        viewporter::ViewporterState,
    },
};
#[cfg(feature = "xwayland")]
use smithay::{
    wayland::xwayland_shell::XWaylandShellState,
    xwayland::{X11Surface, X11Wm},
};
use tracing::{debug, error, info};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{DisplayHandle, protocol::wl_surface::WlSurface};

struct ManagedWindow {
    id: u64,
    surface: ManagedSurface,
    position: CanvasPoint,
    kind: ManagedWindowKind,
    shell_output_name: Option<String>,
    decoration: WindowDecoration,
    /// Masonry-rasterized title-bar image (background, title, close button),
    /// lazily built and cached. See [`masonry_titlebar`].
    titlebar: Option<masonry_titlebar::TitlebarBuffer>,
    /// Bounding-box size of the window's surface tree, cached on commit so the
    /// per-frame rendering and hit-testing paths don't re-walk the tree.
    content_bbox_size: Size<i32, Logical>,
    mapped: bool,
    initial_placement_done: bool,
    initial_output_name: Option<String>,
    presentation: presentation::WindowPresentation,
    pending_resize: Option<windows::PendingResize>,
    /// Fractional scale most recently advertised to this window's surface tree.
    /// Compared each frame so the preferred scale is only re-sent when the
    /// window moves onto an output with a different scale (e.g. dragged across
    /// monitors). `0.0` is a sentinel meaning "nothing advertised yet".
    advertised_fractional_scale: f64,
}

#[derive(Clone)]
enum ManagedSurface {
    Xdg(ToplevelSurface),
    #[cfg(feature = "xwayland")]
    X11(Box<X11Surface>),
}

impl ManagedSurface {
    fn wl_surface(&self) -> Option<WlSurface> {
        match self {
            Self::Xdg(surface) => Some(surface.wl_surface().clone()),
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => surface.wl_surface(),
        }
    }

    fn xdg_toplevel(&self) -> Option<&ToplevelSurface> {
        match self {
            Self::Xdg(surface) => Some(surface),
            #[cfg(feature = "xwayland")]
            Self::X11(_) => None,
        }
    }

    fn send_close(&self) {
        match self {
            Self::Xdg(surface) => surface.send_close(),
            #[cfg(feature = "xwayland")]
            Self::X11(surface) => {
                if let Err(error) = surface.close() {
                    tracing::warn!(%error, "failed to close X11 window");
                }
            }
        }
    }

    fn same_xdg(&self, other: &ToplevelSurface) -> bool {
        match self {
            Self::Xdg(surface) => surface == other,
            #[cfg(feature = "xwayland")]
            Self::X11(_) => false,
        }
    }

    #[cfg(feature = "xwayland")]
    fn same_x11(&self, other: &X11Surface) -> bool {
        match self {
            Self::Xdg(_) => false,
            Self::X11(surface) => surface.as_ref() == other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedWindowKind {
    Normal,
    DesktopItem,
    ShellBar,
    /// The launcher palette: a transient shell surface the shell opens below the
    /// bar to show app-search results as a dropdown.
    Launcher,
    /// Invisible shell-owned surface that hosts the shell's output watcher task.
    ShellWatcher,
    /// Passive hardware feedback: screen-space, never an input/focus target.
    ControlOsd,
}

impl ManagedWindowKind {
    /// Whether this kind is shell chrome (the bar or the launcher palette),
    /// which is rendered in screen space without server-side decorations rather
    /// than as a normal canvas window.
    fn is_shell_chrome(self) -> bool {
        matches!(
            self,
            Self::ShellBar | Self::Launcher | Self::ShellWatcher | Self::ControlOsd
        )
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
    visible_start: Point<i32, Logical>,
}

#[derive(Clone, Copy)]
struct ResizeState {
    window_id: u64,
    edges: ResizeEdges,
    pointer_start: Point<f64, Logical>,
    initial_content_size: Size<i32, Logical>,
    initial_bounds: smithay::utils::Rectangle<i32, Logical>,
    moved: bool,
}

enum HitTarget {
    TitlebarButton {
        window_index: usize,
        button: windows::TitlebarButton,
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
    display_handle: DisplayHandle,
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    _xdg_decoration_state: XdgDecorationState,
    _output_manager_state: OutputManagerState,
    /// `wp_viewporter`: lets clients declare a surface's logical destination
    /// size independently of its buffer, which HiDPI toolkits (GTK/Firefox) use
    /// together with fractional scale to avoid rendering an oversized buffer.
    _viewporter_state: ViewporterState,
    /// `wp_fractional_scale_v1`: advertises the preferred scale to clients so
    /// they render buffers at the monitor scale instead of falling back to a
    /// mismatched integer buffer scale.
    _fractional_scale_manager_state: FractionalScaleManagerState,
    #[cfg(feature = "xwayland")]
    _xwayland_shell_state: XWaylandShellState,
    shm_state: ShmState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    primary_selection_state: PrimarySelectionState,
    session_lock: session_lock::LockState,
    remote_input: input::RemoteInput,
    remote_capture: capture::protocol::RemoteCaptureState,
    gtk_shell: gtk_shell::GtkShellState,
    _seat: Seat<Self>,
    pointer: PointerHandle<Self>,
    touch: TouchHandle<Self>,
    ui_touch: Option<input::UiTouch>,
    titlebar_input: input::TitlebarInput,
    focused_shell_peek: focused_shell_peek::FocusedShellPeek,
    pressed_pointer_buttons: HashSet<u32>,
    keyboard: KeyboardHandle<Self>,
    viewport_offset: CanvasPoint,
    layout_store: layout::LayoutStore,
    layout_editor: layout_editor::State,
    power_confirmation: Option<windows::PowerConfirmation>,
    desktop: desktop::State,
    canvas_peek: canvas_peek::State,
    canvas_search: canvas_search::State,
    canvas_preview: canvas_preview::State,
    viewport_scale: f64,
    viewport_animation: Option<ViewportAnimation>,
    window_switcher: switcher::WindowSwitcher,
    window_group: window_group::WindowGroup,
    switcher_input: input::SwitcherInput,
    pinch_zoom: input::PinchZoom,
    windows: windows::State,
    idle_daemon: WindowIdleDaemon,
    settings_subscription: settings_watch::Subscription,
    titlebar_icons: masonry_titlebar::icons::State,
    focused_normal_window_id: Option<u64>,
    shortcuts: input::KeyboardShortcuts,
    search_request: shell_integration::search::SearchRequest,
    control_events: shell_integration::device_control::ControlEvents,
    favorite_drawing: input::FavoriteDrawing,
    drag: Option<DragState>,
    resize: Option<ResizeState>,
    /// Cursor the compositor wants the host winit window to show. Updated from
    /// pointer-motion hit-testing and applied to the backend by the event loop
    /// (which owns the winit window, a sibling of this handler state).
    cursor_icon: CursorIcon,
    cursor_visible: bool,
    /// Client-provided cursor surface set through `wl_pointer.set_cursor`, when a
    /// focused client wants to draw its own cursor (e.g. Firefox's text I-beam).
    /// When present it takes precedence over `cursor_icon` and is presented as a
    /// software cursor with the client-supplied hotspot.
    cursor_surface: Option<WlSurface>,
    next_spawn_output_name: Option<String>,
    snap_wayland_socket_paths: HashSet<std::path::PathBuf>,
    pointer_location: Point<f64, Logical>,
    raw_pointer_location: Point<f64, Logical>,
    touch_input: input::TouchTracker,
    dispatching_touch: bool,
    touch_pointer_pressed: bool,
    scroll_zooms_without_super: bool,
    outputs: OutputSet,
    app_catalog: AppCatalog,
    needs_redraw: bool,
    dmabuf_state: DmabufState,
    _dmabuf_global: DmabufGlobal,
    pending_dmabuf_imports: Vec<(Dmabuf, ImportNotifier)>,
    loop_handle: LoopHandle<'static, CalloopData>,
    popups: PopupManager,
    popup_grab: Option<smithay::desktop::PopupGrab<App>>,
    background_mode: hearthspace_ipc::BackgroundMode,
    appearance: hearthspace_ipc::Appearance,
    palette: hearthspace_ui_components::Palette,
    background_image_path: Option<String>,
    /// Cached texture for the repeated background image tile.
    background_image_tile: Option<rendering::BackgroundImageTile>,
    corner_blur: rendering::CornerBlurCache,
    window_backdrop: rendering::WindowBackdropCache,
    window_frames: window_frame::WindowFrameCache,
    /// Stable render-element ids for visible background tile instances. Reusing
    /// ids across frames keeps the damage tracker from treating every tile as
    /// new on each redraw.
    background_tile_ids: Vec<Id>,
    software_cursor_visible: bool,
    software_cursor: SoftwareCursor,
    #[cfg(feature = "xwayland")]
    xwm: Option<X11Wm>,
    #[cfg(feature = "xwayland")]
    xdisplay: Option<u32>,
}

#[derive(Debug, Default)]
struct SettingsRefresh {
    output_layout_changed: bool,
    background_changed: bool,
    appearance_changed: bool,
}

impl App {
    fn apply_settings(&mut self, settings: hearthspace_ipc::Settings) -> SettingsRefresh {
        let mut refresh = SettingsRefresh {
            output_layout_changed: self
                .set_output_layout(OutputLayout::from_monitors(settings.monitors)),
            ..SettingsRefresh::default()
        };
        if refresh.output_layout_changed {
            self.configure_shell_bars();
            self.reconcile_pointer_after_output_geometry_change();
            self.refresh_fractional_scales();
        }

        if self.background_mode != settings.background_mode {
            self.background_mode = settings.background_mode;
            refresh.background_changed = true;
        }
        if self.background_image_path != settings.background_image_path {
            self.background_image_path = settings.background_image_path;
            self.background_image_tile = None;
            refresh.background_changed = true;
        }
        if self.appearance != settings.appearance {
            refresh.appearance_changed = true;
            if self.appearance.color_mode != settings.appearance.color_mode {
                refresh.background_changed |= self.invalidate_default_background();
            }
            self.appearance = settings.appearance;
            self.palette = hearthspace_ui_components::Palette::new(self.appearance);
            #[cfg(feature = "xwayland")]
            self.publish_xsettings();
        }
        if refresh.background_changed {
            // Static wallpaper elements have no content commit counter. New
            // identities invalidate output and corner-backdrop damage trackers
            // even when the replacement image has identical geometry.
            self.background_tile_ids.clear();
        }

        refresh
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
        WinitEvent::Input(event) => input::handle_winit_input(&mut data.state, event),
        WinitEvent::Redraw => data.state.request_redraw(),
        WinitEvent::CloseRequested => {
            info!("winit close requested; stopping compositor event loop");
            data.running = false;
        }
        WinitEvent::Focus(false) => data.state.cancel_input_gestures(),
        WinitEvent::Focus(true) => {}
    })?;

    let wayland_display = crate::config::wayland_display_name();
    info!(wayland_display, "Hearthspace compositor running");
    if state.scroll_zooms_without_super {
        info!("scroll zoom testing mode enabled: vertical scroll zooms without Super");
    }

    let mut data = CalloopData {
        captures: capture::CaptureState::default(),
        state,
        display,
        backend: Backend::Winit(Box::new(backend)),
        damage_tracker: OutputDamageTracker::new(output_size, 1.0, Transform::Flipped180),
        start_time: Instant::now(),
        running: true,
        exit_at: options.exit_after.map(|duration| Instant::now() + duration),
        full_redraw: 1,
        applied_cursor: CursorIcon::Default,
        applied_cursor_visible: true,
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

    let wayland_display = crate::config::wayland_display_name();
    info!(wayland_display, "headless Hearthspace compositor running");
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
    let viewporter_state = ViewporterState::new::<App>(&dh);
    let fractional_scale_manager_state = FractionalScaleManagerState::new::<App>(&dh);
    #[cfg(feature = "xwayland")]
    let xwayland_shell_state = XWaylandShellState::new::<App>(&dh);
    let shm_state = ShmState::new::<App>(&dh, vec![]);
    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "hearthspace");
    let keyboard = seat.add_keyboard(
        Default::default(),
        KEYBOARD_REPEAT_DELAY_MS,
        KEYBOARD_REPEAT_RATE,
    )?;
    let pointer = seat.add_pointer();
    let touch = seat.add_touch();

    let event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let command_socket_path =
        register_common_event_sources(&mut display, &handle, termination_signals)?;

    let app_catalog = AppCatalog::load();
    debug!(
        app_count = app_catalog.apps().len(),
        "loaded app catalog for compositor"
    );
    let settings = match hearthspace_ipc::client_get_settings() {
        Ok(settings) => settings,
        Err(error) => {
            debug!(%error, "using default compositor settings because settingsd is unavailable");
            hearthspace_ipc::Settings::default()
        }
    };
    let output_layout = OutputLayout::from_monitors(settings.monitors.clone());

    let layout_store = layout::LayoutStore::from_environment(matches!(
        crate::selected_backend(options.backend),
        crate::BackendSelection::Udev
    ));
    let (viewport_offset, viewport_scale) = layout_store.viewport();
    let frame_layouts = layout_store.frame_layouts();
    let allow_hide_tools = layout_store.allow_hide_tools();
    let peek_homes = layout_store.peek_homes();
    let app = App {
        display_handle: dh.clone(),
        compositor_state,
        xdg_shell_state: XdgShellState::new_with_capabilities::<App>(
            &dh,
            [
                xdg_toplevel::WmCapabilities::Maximize,
                xdg_toplevel::WmCapabilities::Fullscreen,
            ],
        ),
        _xdg_decoration_state: xdg_decoration_state,
        _output_manager_state: output_manager_state,
        _viewporter_state: viewporter_state,
        _fractional_scale_manager_state: fractional_scale_manager_state,
        #[cfg(feature = "xwayland")]
        _xwayland_shell_state: xwayland_shell_state,
        shm_state,
        seat_state,
        data_device_state: DataDeviceState::new::<App>(&dh),
        primary_selection_state: PrimarySelectionState::new::<App>(&dh),
        remote_input: input::RemoteInput::new(&dh, options.remote_desktop),
        remote_capture: capture::protocol::RemoteCaptureState::new(
            &dh,
            options.remote_desktop || options.screen_cast,
        ),
        gtk_shell: gtk_shell::GtkShellState::new(&dh),
        session_lock: session_lock::LockState::new(
            &dh,
            !matches!(
                crate::selected_backend(options.backend),
                crate::BackendSelection::Winit
            ),
        ),
        _seat: seat,
        pointer,
        touch,
        ui_touch: None,
        titlebar_input: input::TitlebarInput::default(),
        focused_shell_peek: focused_shell_peek::FocusedShellPeek::default(),
        pressed_pointer_buttons: HashSet::new(),
        keyboard,
        viewport_offset,
        viewport_scale,
        layout_store,
        layout_editor: layout_editor::State::from_layouts(frame_layouts, allow_hide_tools),
        power_confirmation: None,
        desktop: Default::default(),
        canvas_peek: canvas_peek::State::with_homes(peek_homes),
        canvas_search: Default::default(),
        canvas_preview: Default::default(),
        viewport_animation: None,
        window_switcher: switcher::WindowSwitcher::default(),
        window_group: window_group::WindowGroup::default(),
        switcher_input: input::SwitcherInput::default(),
        pinch_zoom: input::PinchZoom::default(),
        windows: windows::State::default(),
        idle_daemon: WindowIdleDaemon::new(WINDOW_IDLE_THRESHOLDS),
        settings_subscription: settings_watch::subscribe(&handle)?,
        titlebar_icons: Default::default(),
        focused_normal_window_id: None,
        shortcuts: input::KeyboardShortcuts::default(),
        search_request: Default::default(),
        control_events: Default::default(),
        favorite_drawing: Default::default(),
        drag: None,
        resize: None,
        cursor_icon: CursorIcon::Default,
        cursor_visible: true,
        cursor_surface: None,
        next_spawn_output_name: None,
        snap_wayland_socket_paths: HashSet::new(),
        pointer_location: (0.0, 0.0).into(),
        raw_pointer_location: (0.0, 0.0).into(),
        touch_input: input::TouchTracker::default(),
        dispatching_touch: false,
        touch_pointer_pressed: false,
        scroll_zooms_without_super: options.scroll_zooms_without_super,
        outputs: OutputSet::new(primary_output, output_layout),
        app_catalog,
        needs_redraw: true,
        dmabuf_state: dmabuf.state,
        _dmabuf_global: dmabuf.global,
        pending_dmabuf_imports: Vec::new(),
        loop_handle: handle.clone(),
        popups: PopupManager::default(),
        popup_grab: None,
        background_mode: settings.background_mode,
        appearance: settings.appearance,
        palette: hearthspace_ui_components::Palette::new(settings.appearance),
        background_image_path: settings.background_image_path,
        background_image_tile: None,
        corner_blur: rendering::CornerBlurCache::default(),
        window_backdrop: rendering::WindowBackdropCache::default(),
        window_frames: window_frame::WindowFrameCache::default(),
        background_tile_ids: Vec::new(),
        software_cursor_visible: false,
        software_cursor: cursor::standard_software_cursor(),
        #[cfg(feature = "xwayland")]
        xwm: None,
        #[cfg(feature = "xwayland")]
        xdisplay: None,
    };

    #[cfg(feature = "xwayland")]
    let mut app = app;
    #[cfg(feature = "xwayland")]
    app.start_xwayland(&dh);

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
    let wayland_display = crate::config::wayland_display_name();
    let socket_source = ListeningSocketSource::with_name(&wayland_display)?;
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
            let active = data.capture_active();
            data.state.set_remote_input_active(active);
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

smithay::delegate_dispatch2!(App);
