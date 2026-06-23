use std::{os::unix::io::OwnedFd, process::Command, sync::Arc};

use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, ButtonState, InputEvent, KeyboardKeyEvent, PointerButtonEvent,
        },
        renderer::{
            element::{
                solid::{SolidColorBuffer, SolidColorRenderElement},
                surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
                Kind,
            },
            gles::GlesRenderer,
            utils::{draw_render_elements, on_commit_buffer_handler},
            Color32F, Frame, Renderer,
        },
        winit::{self, WinitEvent},
    },
    delegate_compositor, delegate_data_device, delegate_seat, delegate_shm, delegate_xdg_shell,
    input::{
        keyboard::{FilterResult, KeyboardHandle},
        pointer::{ButtonEvent, MotionEvent, PointerHandle},
        Seat, SeatHandler, SeatState,
    },
    reexports::{
        wayland_server::{protocol::wl_seat, Display},
        winit::platform::pump_events::PumpStatus,
    },
    utils::{Logical, Physical, Point, Rectangle, Serial, Size, Transform},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            with_surface_tree_downward, CompositorClientState, CompositorHandler, CompositorState,
            SurfaceAttributes, TraversalAction,
        },
        selection::{
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
            SelectionHandler,
        },
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        },
        shm::{ShmHandler, ShmState},
    },
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::{
        wl_buffer,
        wl_surface::{self, WlSurface},
    },
    Client, ListeningSocket,
};

const WAYLAND_DISPLAY_NAME: &str = "wayland-hearthspace-0";
const DEFAULT_APP: &str = "foot";
const CONTROL_BAR_HEIGHT: i32 = 48;
const BUTTON_Y: i32 = 8;
const BUTTON_HEIGHT: i32 = 32;
const BUTTON_GAP: i32 = 8;

#[derive(Debug, Clone, Copy)]
struct CanvasPoint {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, Copy)]
enum ControlAction {
    SpawnApp,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
}

#[derive(Debug, Clone, Copy)]
struct ControlButton {
    action: ControlAction,
    rect: Rectangle<i32, Logical>,
}

struct App {
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    shm_state: ShmState,
    seat_state: SeatState<Self>,
    data_device_state: DataDeviceState,
    _seat: Seat<Self>,
    pointer: PointerHandle<Self>,
    keyboard: KeyboardHandle<Self>,
    viewport_offset: CanvasPoint,
    window_positions: Vec<CanvasPoint>,
    next_spawn_position: CanvasPoint,
    pointer_location: Point<f64, Logical>,
    output_size: Size<i32, Physical>,
}

impl BufferHandler for App {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl XdgShellHandler for App {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.window_positions.push(self.next_spawn_position);
        self.next_spawn_position.x += 48;
        self.next_spawn_position.y += 48;

        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Activated);
        });
        surface.send_configure();
    }

    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}

    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {
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
    }
}

impl ShmHandler for App {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    run_winit()
}

fn run_winit() -> Result<(), Box<dyn std::error::Error>> {
    let mut display: Display<App> = Display::new()?;
    let dh = display.handle();

    let compositor_state = CompositorState::new::<App>(&dh);
    let shm_state = ShmState::new::<App>(&dh, vec![]);
    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "hearthspace");
    let keyboard = seat.add_keyboard(Default::default(), 200, 200)?;
    let pointer = seat.add_pointer();

    let mut state = App {
        compositor_state,
        xdg_shell_state: XdgShellState::new::<App>(&dh),
        shm_state,
        seat_state,
        data_device_state: DataDeviceState::new::<App>(&dh),
        _seat: seat,
        pointer,
        keyboard,
        viewport_offset: CanvasPoint { x: 0, y: 0 },
        window_positions: Vec::new(),
        next_spawn_position: CanvasPoint { x: 80, y: 96 },
        pointer_location: (0.0, 0.0).into(),
        output_size: (1, 1).into(),
    };

    let listener = ListeningSocket::bind(WAYLAND_DISPLAY_NAME)?;
    let mut clients = Vec::new();
    let (mut backend, mut winit) = winit::init::<GlesRenderer>()?;
    let start_time = std::time::Instant::now();

    println!("Hearthspace running on WAYLAND_DISPLAY={WAYLAND_DISPLAY_NAME}");

    loop {
        let status = winit.dispatch_new_events(|event| match event {
            WinitEvent::Resized { .. } => {}
            WinitEvent::Input(event) => handle_input_event(&mut state, event),
            _ => (),
        });

        match status {
            PumpStatus::Continue => (),
            PumpStatus::Exit(_) => return Ok(()),
        };

        state.output_size = backend.window_size();
        let damage = Rectangle::from_size(state.output_size);
        {
            let (renderer, mut framebuffer) = backend.bind()?;
            let window_elements = state.window_render_elements(renderer);
            let control_elements = state.control_bar_elements();

            let mut frame =
                renderer.render(&mut framebuffer, state.output_size, Transform::Flipped180)?;
            frame.clear(Color32F::new(0.04, 0.05, 0.07, 1.0), &[damage])?;
            draw_render_elements::<GlesRenderer, _, _>(
                &mut frame,
                1.0,
                &window_elements,
                &[damage],
            )?;
            draw_render_elements::<GlesRenderer, _, _>(
                &mut frame,
                1.0,
                &control_elements,
                &[damage],
            )?;
            let _ = frame.finish()?;

            for surface in state.xdg_shell_state.toplevel_surfaces() {
                send_frames_surface_tree(
                    surface.wl_surface(),
                    start_time.elapsed().as_millis() as u32,
                );
            }

            if let Some(stream) = listener.accept()? {
                println!("Got a client: {stream:?}");
                let client = display
                    .handle()
                    .insert_client(stream, Arc::new(ClientState::default()))?;
                clients.push(client);
            }

            display.dispatch_clients(&mut state)?;
            display.flush_clients()?;
        }

        backend.submit(Some(&[damage]))?;
    }
}

fn handle_input_event(state: &mut App, event: InputEvent<smithay::backend::winit::WinitInput>) {
    let time = 0;
    match event {
        InputEvent::Keyboard { event } => {
            let keyboard = state.keyboard.clone();
            keyboard.input::<(), _>(
                state,
                event.key_code(),
                event.state(),
                Serial::from(0),
                time,
                |_, _, _| FilterResult::Forward,
            );
        }
        InputEvent::PointerMotionAbsolute { event } => {
            let location = event.position_transformed(state.output_size.to_logical(1));
            state.pointer_location = location;
            let focus = state.surface_under(location);
            let pointer = state.pointer.clone();
            pointer.motion(
                state,
                focus,
                &MotionEvent {
                    location,
                    serial: Serial::from(0),
                    time,
                },
            );
            pointer.frame(state);
        }
        InputEvent::PointerButton { event } => {
            if event.state() == ButtonState::Pressed {
                if let Some(action) = state.control_action_at(state.pointer_location) {
                    state.run_control_action(action);
                    return;
                }
            }

            let focus = state.surface_under(state.pointer_location);
            if event.state() == ButtonState::Pressed {
                if let Some((surface, _)) = focus.clone() {
                    let keyboard = state.keyboard.clone();
                    keyboard.set_focus(state, Some(surface), Serial::from(0));
                }
            }

            let pointer = state.pointer.clone();
            pointer.button(
                state,
                &ButtonEvent {
                    serial: Serial::from(0),
                    time,
                    button: event.button_code(),
                    state: event.state(),
                },
            );
            pointer.frame(state);
        }
        _ => {}
    }
}

impl App {
    fn window_render_elements(
        &self,
        renderer: &mut GlesRenderer,
    ) -> Vec<WaylandSurfaceRenderElement<GlesRenderer>> {
        self.xdg_shell_state
            .toplevel_surfaces()
            .iter()
            .enumerate()
            .flat_map(|(index, surface)| {
                let position = self.window_canvas_position(index);
                let screen_position = (
                    position.x - self.viewport_offset.x,
                    position.y - self.viewport_offset.y,
                );
                render_elements_from_surface_tree(
                    renderer,
                    surface.wl_surface(),
                    screen_position,
                    1.0,
                    1.0,
                    Kind::Unspecified,
                )
            })
            .collect()
    }

    fn control_bar_elements(&self) -> Vec<SolidColorRenderElement> {
        let mut elements = vec![solid_element(
            Rectangle::new(
                (0, 0).into(),
                (self.output_size.w, CONTROL_BAR_HEIGHT).into(),
            ),
            Color32F::new(0.10, 0.12, 0.16, 0.96),
        )];

        for button in self.control_buttons() {
            elements.push(solid_element(
                button.rect,
                Color32F::new(0.22, 0.28, 0.38, 1.0),
            ));

            for icon_rect in icon_rects(button) {
                elements.push(solid_element(
                    icon_rect,
                    Color32F::new(0.84, 0.90, 1.0, 1.0),
                ));
            }
        }

        elements
    }

    fn control_buttons(&self) -> Vec<ControlButton> {
        let specs = [
            (ControlAction::SpawnApp, 112),
            (ControlAction::PanLeft, 64),
            (ControlAction::PanRight, 64),
            (ControlAction::PanUp, 64),
            (ControlAction::PanDown, 64),
        ];
        let mut x = BUTTON_GAP;

        specs
            .into_iter()
            .map(|(action, width)| {
                let rect = Rectangle::new((x, BUTTON_Y).into(), (width, BUTTON_HEIGHT).into());
                x += width + BUTTON_GAP;
                ControlButton { action, rect }
            })
            .collect()
    }

    fn control_action_at(&self, point: Point<f64, Logical>) -> Option<ControlAction> {
        self.control_buttons()
            .into_iter()
            .find(|button| rect_contains(button.rect, point))
            .map(|button| button.action)
    }

    fn run_control_action(&mut self, action: ControlAction) {
        match action {
            ControlAction::SpawnApp => self.spawn_app(),
            ControlAction::PanLeft => self.viewport_offset.x -= self.output_size.w / 2,
            ControlAction::PanRight => self.viewport_offset.x += self.output_size.w / 2,
            ControlAction::PanUp => self.viewport_offset.y -= self.output_size.h / 2,
            ControlAction::PanDown => self.viewport_offset.y += self.output_size.h / 2,
        }
    }

    fn spawn_app(&mut self) {
        match Command::new(DEFAULT_APP)
            .env("WAYLAND_DISPLAY", WAYLAND_DISPLAY_NAME)
            .spawn()
        {
            Ok(_) => {}
            Err(error) => eprintln!("Failed to spawn {DEFAULT_APP}: {error}"),
        }
    }

    fn surface_under(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        if location.y < f64::from(CONTROL_BAR_HEIGHT) {
            return None;
        }

        self.xdg_shell_state
            .toplevel_surfaces()
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, surface)| {
                let position = self.window_canvas_position(index);
                let origin = Point::<i32, Logical>::from((
                    position.x - self.viewport_offset.x,
                    position.y - self.viewport_offset.y,
                ))
                .to_f64();
                Some((surface.wl_surface().clone(), origin))
            })
    }

    fn window_canvas_position(&self, index: usize) -> CanvasPoint {
        self.window_positions
            .get(index)
            .copied()
            .unwrap_or(CanvasPoint { x: 80, y: 96 })
    }
}

fn rect_contains(rect: Rectangle<i32, Logical>, point: Point<f64, Logical>) -> bool {
    let min_x = f64::from(rect.loc.x);
    let min_y = f64::from(rect.loc.y);
    let max_x = f64::from(rect.loc.x + rect.size.w);
    let max_y = f64::from(rect.loc.y + rect.size.h);

    point.x >= min_x && point.x < max_x && point.y >= min_y && point.y < max_y
}

fn solid_element(rect: Rectangle<i32, Logical>, color: Color32F) -> SolidColorRenderElement {
    let buffer = SolidColorBuffer::new(rect.size, color);
    SolidColorRenderElement::from_buffer(
        &buffer,
        rect.loc.to_physical(1),
        1.0,
        1.0,
        Kind::Unspecified,
    )
}

fn icon_rects(button: ControlButton) -> Vec<Rectangle<i32, Logical>> {
    let center_x = button.rect.loc.x + button.rect.size.w / 2;
    let center_y = button.rect.loc.y + button.rect.size.h / 2;

    match button.action {
        ControlAction::SpawnApp => vec![
            Rectangle::new((center_x - 10, center_y - 2).into(), (20, 4).into()),
            Rectangle::new((center_x - 2, center_y - 10).into(), (4, 20).into()),
        ],
        ControlAction::PanLeft => vec![
            Rectangle::new((center_x - 4, center_y - 2).into(), (18, 4).into()),
            Rectangle::new((center_x - 12, center_y - 2).into(), (8, 4).into()),
            Rectangle::new((center_x - 8, center_y - 6).into(), (4, 4).into()),
            Rectangle::new((center_x - 4, center_y - 10).into(), (4, 4).into()),
            Rectangle::new((center_x - 8, center_y + 2).into(), (4, 4).into()),
            Rectangle::new((center_x - 4, center_y + 6).into(), (4, 4).into()),
        ],
        ControlAction::PanRight => vec![
            Rectangle::new((center_x - 14, center_y - 2).into(), (18, 4).into()),
            Rectangle::new((center_x + 4, center_y - 2).into(), (8, 4).into()),
            Rectangle::new((center_x + 4, center_y - 6).into(), (4, 4).into()),
            Rectangle::new((center_x, center_y - 10).into(), (4, 4).into()),
            Rectangle::new((center_x + 4, center_y + 2).into(), (4, 4).into()),
            Rectangle::new((center_x, center_y + 6).into(), (4, 4).into()),
        ],
        ControlAction::PanUp => vec![
            Rectangle::new((center_x - 2, center_y - 4).into(), (4, 18).into()),
            Rectangle::new((center_x - 2, center_y - 12).into(), (4, 8).into()),
            Rectangle::new((center_x - 6, center_y - 8).into(), (4, 4).into()),
            Rectangle::new((center_x - 10, center_y - 4).into(), (4, 4).into()),
            Rectangle::new((center_x + 2, center_y - 8).into(), (4, 4).into()),
            Rectangle::new((center_x + 6, center_y - 4).into(), (4, 4).into()),
        ],
        ControlAction::PanDown => vec![
            Rectangle::new((center_x - 2, center_y - 14).into(), (4, 18).into()),
            Rectangle::new((center_x - 2, center_y + 4).into(), (4, 8).into()),
            Rectangle::new((center_x - 6, center_y + 4).into(), (4, 4).into()),
            Rectangle::new((center_x - 10, center_y).into(), (4, 4).into()),
            Rectangle::new((center_x + 2, center_y + 4).into(), (4, 4).into()),
            Rectangle::new((center_x + 6, center_y).into(), (4, 4).into()),
        ],
    }
}

fn send_frames_surface_tree(surface: &wl_surface::WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
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
