use std::{
    os::unix::{io::OwnedFd, net::UnixListener as CommandListener},
    sync::Arc,
};

use crate::{RunOptions, config::*, geometry::CanvasPoint, shell::app_catalog::AppCatalog};

mod idle;
mod input;
mod rendering;
mod shell_integration;
mod viewport;
mod windows;

use idle::{ActivityReason, IdleTransition, WindowIdleDaemon};
use input::handle_input_event;
use rendering::send_frames_surface_tree;
use shell_integration::{
    accept_command_connections, command_socket_path, remove_stale_socket, spawn_shell_bar,
};
use viewport::ViewportAnimation;
use windows::{decoration_for_new_window, position_for_new_window, window_kind_for_toplevel};

use smithay::{
    backend::{
        renderer::{
            gles::GlesRenderer,
            utils::on_commit_buffer_handler,
        },
        winit::{self, WinitEvent},
    },
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm,
    delegate_xdg_decoration, delegate_xdg_shell,
    input::{Seat, SeatHandler, SeatState, keyboard::KeyboardHandle, pointer::PointerHandle},
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        calloop::{
            EventLoop, Interest, Mode as CalloopMode, PostAction, generic::Generic,
        },
        wayland_server::{Display, protocol::wl_seat},
    },
    utils::{Logical, Physical, Point, Rectangle, Serial, Size, Transform},
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorClientState, CompositorHandler, CompositorState},
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
    Client, DisplayHandle,
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::{wl_buffer, wl_surface::WlSurface},
};

struct ManagedWindow {
    id: u64,
    surface: ToplevelSurface,
    position: CanvasPoint,
    kind: ManagedWindowKind,
    decoration: WindowDecoration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagedWindowKind {
    Normal,
    ShellBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowDecoration {
    ServerSide,
    ClientSide,
}

struct DragState {
    window_index: usize,
    pointer_start: Point<f64, Logical>,
    window_start: CanvasPoint,
}

enum HitTarget {
    CloseButton {
        window_index: usize,
    },
    TitleBar {
        window_index: usize,
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
    next_spawn_position: CanvasPoint,
    spawn_offset: i32,
    pointer_location: Point<f64, Logical>,
    scroll_zooms_without_super: bool,
    output_size: Size<i32, Physical>,
    output: Output,
    app_catalog: AppCatalog,
    needs_redraw: bool,
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
        });
        if kind == ManagedWindowKind::Normal {
            self.idle_daemon.register_window(id);
        }
        self.output.enter(surface.wl_surface());
        self.request_redraw();

        self.configure_toplevel(&surface, kind);
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}

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
            window_index,
            pointer_start: self.pointer_location,
            window_start: self.windows[window_index].position,
        });
        self.request_redraw();
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
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
            if kind == ManagedWindowKind::ShellBar {
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
                ManagedWindowKind::ShellBar => self.windows.len(),
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

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
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

    let (backend, winit) = winit::init::<GlesRenderer>()?;
    let output_size = backend.window_size();
    let output = create_output(&dh, output_size);

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
        next_spawn_position: CanvasPoint { x: 80, y: 96 },
        spawn_offset: 0,
        pointer_location: (0.0, 0.0).into(),
        scroll_zooms_without_super: options.scroll_zooms_without_super,
        output_size,
        output,
        app_catalog: AppCatalog::load(),
        needs_redraw: true,
    };

    let command_socket_path = command_socket_path();
    remove_stale_socket(&command_socket_path)?;
    let command_listener = CommandListener::bind(&command_socket_path)?;
    command_listener.set_nonblocking(true)?;

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let handle = event_loop.handle();

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
                data.state.configure_shell_bars();
                data.state.request_redraw();
            }
        }
        WinitEvent::Input(event) => handle_input_event(&mut data.state, event),
        WinitEvent::Redraw => data.state.request_redraw(),
        WinitEvent::CloseRequested => data.running = false,
        WinitEvent::Focus(_) => {}
    })?;

    spawn_shell_bar(&command_socket_path);

    println!("Hearthspace running on WAYLAND_DISPLAY={WAYLAND_DISPLAY_NAME}");
    if state.scroll_zooms_without_super {
        println!("Scroll zoom testing mode enabled: vertical scroll zooms without Super");
    }

    let mut data = CalloopData {
        state,
        display,
        backend: Backend::Winit(backend),
        start_time: std::time::Instant::now(),
        running: true,
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

        data.state.handle_idle_transitions();
        data.state.advance_viewport_animation();

        if data.state.needs_redraw {
            data.render()?;
            data.state.needs_redraw = false;
        }

        data.display.flush_clients()?;
        data.state.output.cleanup();
    }

    Ok(())
}

enum Backend {
    Winit(smithay::backend::winit::WinitGraphicsBackend<GlesRenderer>),
}

struct CalloopData {
    state: App,
    display: Display<App>,
    backend: Backend,
    start_time: std::time::Instant,
    running: bool,
}

impl CalloopData {
    fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let CalloopData {
            state,
            display,
            backend,
            start_time,
            ..
        } = self;
        let Backend::Winit(backend) = backend;

        let damage = Rectangle::from_size(state.output_size);
        {
            let (renderer, mut framebuffer) = backend.bind()?;
            state.render_frame(renderer, &mut framebuffer, state.output_size)?;
        }

        for window in &state.windows {
            send_frames_surface_tree(
                window.surface.wl_surface(),
                start_time.elapsed().as_millis() as u32,
            );
        }

        display.flush_clients()?;
        backend.submit(Some(&[damage]))?;
        Ok(())
    }
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
