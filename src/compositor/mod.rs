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
mod rendering;
mod shell_integration;
#[cfg(feature = "udev")]
mod udev;
mod viewport;
mod windows;

use idle::{IdleTransition, WindowIdleDaemon};
#[cfg(any(feature = "winit", feature = "udev"))]
pub(in crate::compositor) use input::handle_input_event;
use rendering::send_frames_surface_tree;
use shell_integration::{
    accept_command_connections, command_socket_path, remove_stale_socket, spawn_shell,
};
use viewport::ViewportAnimation;
use windows::ResizeEdges;

#[cfg(feature = "udev")]
pub use udev::run_udev;

use cursor::CursorIcon;
#[cfg(feature = "winit")]
use smithay::backend::winit::{self, WinitEvent};
use smithay::{
    backend::{
        allocator::dmabuf::Dmabuf,
        allocator::{Format, Fourcc},
        egl::{EGLContext, EGLDevice, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::{
            Bind, ExportMem, ImportDma, Offscreen,
            damage::OutputDamageTracker,
            element::Id,
            gles::{GlesRenderbuffer, GlesRenderer},
        },
    },
    delegate_compositor, delegate_data_device, delegate_dmabuf, delegate_output, delegate_seat,
    delegate_shm, delegate_xdg_decoration, delegate_xdg_shell,
    desktop::PopupManager,
    input::{Seat, SeatState, keyboard::KeyboardHandle, pointer::PointerHandle},
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        calloop::{
            EventLoop, Interest, LoopHandle, Mode as CalloopMode, PostAction, generic::Generic,
        },
        wayland_server::Display,
    },
    utils::{Buffer as BufferCoord, Logical, Physical, Point, Rectangle, Size, Transform},
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

#[cfg(feature = "winit")]
pub fn run_winit(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let display: Display<App> = Display::new()?;
    let dh = display.handle();

    let (mut backend, winit) = winit::init::<GlesRenderer>()?;
    let output_size = backend.window_size();
    let output = create_output(&dh, output_size, 1);

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
    } = initialize_app(display, options, output_size, output, dmabuf)?;

    // Winit drives input and resize events; it is itself a calloop event source.
    handle.insert_source(winit, |event, _, data| match event {
        WinitEvent::Resized { size, .. } => {
            if size != data.state.output_size {
                data.state.output_size = size;
                update_output_mode(&data.state.output, size, 1);
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

    run_event_loop(event_loop, &mut data)
}

pub fn run_headless(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
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
    let buffer_size = Size::<i32, BufferCoord>::from((output_size.w, output_size.h));
    let buffer = renderer.create_buffer(Fourcc::Abgr8888, buffer_size)?;
    let output = create_output(&dh, output_size, output_scale);

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
    } = initialize_app(display, options, output_size, output, dmabuf)?;

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

    run_event_loop(event_loop, &mut data)
}

pub(in crate::compositor) fn create_dmabuf_global(
    dh: &DisplayHandle,
    formats: Vec<Format>,
    main_device: Option<u64>,
) -> Result<DmabufSetup, Box<dyn std::error::Error>> {
    let mut state = DmabufState::new();
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
    output_size: Size<i32, Physical>,
    output: Output,
    dmabuf: DmabufSetup,
) -> Result<AppInit, Box<dyn std::error::Error>> {
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
    let command_socket_path = register_common_event_sources(&mut display, &handle)?;

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
        scroll_zooms_without_super: options.scroll_zooms_without_super,
        output_size,
        output,
        app_catalog: AppCatalog::load(),
        needs_redraw: true,
        dmabuf_state: dmabuf.state,
        _dmabuf_global: dmabuf.global,
        pending_dmabuf_imports: Vec::new(),
        loop_handle: handle.clone(),
        popups: PopupManager::default(),
        background_dot_ids: Vec::new(),
    };

    if options.start_shell {
        spawn_shell(&command_socket_path);
    }

    Ok(AppInit {
        display,
        event_loop,
        handle,
        app,
    })
}

#[cfg(feature = "udev")]
pub(in crate::compositor) fn create_calloop_data(
    state: App,
    display: Display<App>,
    backend: Backend,
    output_size: Size<i32, Physical>,
) -> CalloopData {
    CalloopData {
        state,
        display,
        backend,
        damage_tracker: OutputDamageTracker::new(output_size, 1.0, Transform::Flipped180),
        start_time: std::time::Instant::now(),
        running: true,
        full_redraw: 1,
        applied_cursor: CursorIcon::Default,
    }
}

fn register_common_event_sources(
    display: &mut Display<App>,
    handle: &LoopHandle<'static, CalloopData>,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
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

    Ok(command_socket_path)
}

fn run_event_loop(
    mut event_loop: EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    data.render()?;

    while data.running {
        // Block until an event arrives; while animating, wake every frame.
        let timeout = data
            .state
            .viewport_animation
            .is_some()
            .then_some(ANIMATION_FRAME_INTERVAL);
        event_loop.dispatch(timeout, data)?;

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
    #[cfg(feature = "winit")]
    Winit(Box<smithay::backend::winit::WinitGraphicsBackend<GlesRenderer>>),
    Headless(Box<HeadlessBackend>),
    #[cfg(feature = "udev")]
    #[allow(dead_code)]
    Udev(Box<udev::UdevBackendState>),
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
        #[cfg(feature = "winit")]
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
                #[cfg(feature = "winit")]
                Backend::Winit(backend) => backend.renderer().import_dmabuf(&dmabuf, None),
                Backend::Headless(backend) => backend.renderer.import_dmabuf(&dmabuf, None),
                #[cfg(feature = "udev")]
                Backend::Udev(backend) => {
                    match backend.import_dmabuf(&dmabuf) {
                        Ok(()) => {
                            let _ = notifier.successful::<App>();
                        }
                        Err(error) => {
                            eprintln!("Failed to import client dmabuf: {error}");
                            notifier.failed();
                        }
                    }
                    continue;
                }
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
            #[cfg(feature = "winit")]
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
            #[cfg(feature = "udev")]
            Backend::Udev(backend) => {
                let force_full_redraw = *full_redraw > 0;
                if force_full_redraw {
                    *full_redraw = full_redraw.saturating_sub(1);
                }
                backend.render_frame(state, damage_tracker, force_full_redraw)?;
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
            #[cfg(feature = "winit")]
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
            #[cfg(feature = "udev")]
            Backend::Udev(_) => {
                return Err("screenshots are not implemented for the udev backend yet".into());
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

pub(in crate::compositor) fn create_output(
    dh: &DisplayHandle,
    size: Size<i32, Physical>,
    scale: i32,
) -> Output {
    create_output_with_properties(
        dh,
        "hearthspace-0".into(),
        PhysicalProperties {
            size: (340, 190).into(),
            subpixel: Subpixel::Unknown,
            make: "Hearthspace".into(),
            model: "Nested Canvas".into(),
        },
        size,
        scale,
        60_000,
    )
}

pub(in crate::compositor) fn create_output_with_properties(
    dh: &DisplayHandle,
    name: String,
    properties: PhysicalProperties,
    size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
) -> Output {
    let output = Output::new(name, properties);
    output.create_global::<App>(dh);
    update_output_mode_with_refresh(&output, size, scale, refresh);
    output
}

#[cfg_attr(not(feature = "winit"), allow(dead_code))]
fn update_output_mode(output: &Output, size: Size<i32, Physical>, scale: i32) {
    update_output_mode_with_refresh(output, size, scale, 60_000);
}

fn update_output_mode_with_refresh(
    output: &Output,
    size: Size<i32, Physical>,
    scale: i32,
    refresh: i32,
) {
    let mode = Mode { size, refresh };
    output.set_preferred(mode);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(Scale::Integer(scale)),
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
