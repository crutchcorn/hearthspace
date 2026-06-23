use std::{os::unix::io::OwnedFd, process::Command, sync::Arc, time::Instant};

use crate::{
    config::*,
    controls::{
        control_buttons as default_control_buttons, label_rects, ControlAction, ControlButton,
    },
    geometry::{
        canvas_to_screen as transform_canvas_to_screen, ease_out_cubic, interpolate_canvas_point,
        interpolate_f64, rect_contains, screen_to_canvas as transform_screen_to_canvas,
        zoom_around_screen_point, CanvasPoint,
    },
};

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
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm,
    delegate_xdg_decoration, delegate_xdg_shell,
    desktop::{
        utils::{bbox_from_surface_tree, under_from_surface_tree},
        WindowSurfaceType,
    },
    input::{
        keyboard::{FilterResult, KeyboardHandle},
        pointer::{ButtonEvent, MotionEvent, PointerHandle},
        Seat, SeatHandler, SeatState,
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
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
        output::{OutputHandler, OutputManagerState},
        selection::{
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
            SelectionHandler,
        },
        shell::xdg::{
            decoration::{XdgDecorationHandler, XdgDecorationState},
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
        },
        shm::{ShmHandler, ShmState},
    },
};
use wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::{
        wl_buffer,
        wl_surface::{self, WlSurface},
    },
    Client, DisplayHandle, ListeningSocket,
};

struct ManagedWindow {
    surface: ToplevelSurface,
    position: CanvasPoint,
}

struct DragState {
    window_index: usize,
    pointer_start: Point<f64, Logical>,
    window_start: CanvasPoint,
}

struct ViewportAnimation {
    from_offset: CanvasPoint,
    from_scale: f64,
    to_offset: CanvasPoint,
    to_scale: f64,
    started_at: Instant,
}

enum HitTarget {
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
    drag: Option<DragState>,
    next_spawn_position: CanvasPoint,
    spawn_offset: i32,
    pointer_location: Point<f64, Logical>,
    output_size: Size<i32, Physical>,
    output: Output,
    control_bar_elements: Vec<SolidColorRenderElement>,
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
        self.windows.push(ManagedWindow {
            surface: surface.clone(),
            position: self.next_spawn_position,
        });
        self.output.enter(surface.wl_surface());
        self.request_redraw();

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

impl XdgDecorationHandler for App {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        configure_server_side_decoration(&toplevel);
        self.request_redraw();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: DecorationMode) {
        configure_server_side_decoration(&toplevel);
        self.request_redraw();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        configure_server_side_decoration(&toplevel);
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

pub fn run_winit() -> Result<(), Box<dyn std::error::Error>> {
    let mut display: Display<App> = Display::new()?;
    let dh = display.handle();

    let compositor_state = CompositorState::new::<App>(&dh);
    let xdg_decoration_state = XdgDecorationState::new::<App>(&dh);
    let output_manager_state = OutputManagerState::new_with_xdg_output::<App>(&dh);
    let shm_state = ShmState::new::<App>(&dh, vec![]);
    let mut seat_state = SeatState::new();
    let mut seat = seat_state.new_wl_seat(&dh, "hearthspace");
    let keyboard = seat.add_keyboard(Default::default(), 200, 200)?;
    let pointer = seat.add_pointer();

    let (mut backend, mut winit) = winit::init::<GlesRenderer>()?;
    let output_size = backend.window_size();
    let output = create_output(&dh, output_size);

    let mut state = App {
        compositor_state,
        xdg_shell_state: XdgShellState::new::<App>(&dh),
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
        drag: None,
        next_spawn_position: CanvasPoint { x: 80, y: 96 },
        spawn_offset: 0,
        pointer_location: (0.0, 0.0).into(),
        output_size,
        output,
        control_bar_elements: Vec::new(),
        needs_redraw: true,
    };
    state.rebuild_control_bar_elements();

    let listener = ListeningSocket::bind(WAYLAND_DISPLAY_NAME)?;
    let mut clients = Vec::new();
    let start_time = std::time::Instant::now();

    println!("Hearthspace running on WAYLAND_DISPLAY={WAYLAND_DISPLAY_NAME}");

    loop {
        let status = winit.dispatch_new_events(|event| match event {
            WinitEvent::Resized { .. } => state.request_redraw(),
            WinitEvent::Input(event) => handle_input_event(&mut state, event),
            _ => (),
        });

        match status {
            PumpStatus::Continue => (),
            PumpStatus::Exit(_) => return Ok(()),
        };

        let output_size = backend.window_size();
        if output_size != state.output_size {
            state.output_size = output_size;
            update_output_mode(&state.output, output_size);
            state.rebuild_control_bar_elements();
            state.request_redraw();
        }

        while let Some(stream) = listener.accept()? {
            println!("Got a client: {stream:?}");
            let client = display
                .handle()
                .insert_client(stream, Arc::new(ClientState::default()))?;
            clients.push(client);
            state.request_redraw();
        }

        display.dispatch_clients(&mut state)?;
        display.flush_clients()?;
        state.output.cleanup();
        state.advance_viewport_animation();

        if state.needs_redraw {
            let damage = Rectangle::from_size(state.output_size);
            {
                let (renderer, mut framebuffer) = backend.bind()?;
                let window_elements = (0..state.windows.len())
                    .map(|index| state.window_render_elements(renderer, index))
                    .collect::<Vec<_>>();
                let title_bar_elements = (0..state.windows.len())
                    .map(|index| state.title_bar_elements(index))
                    .collect::<Vec<_>>();

                let mut frame =
                    renderer.render(&mut framebuffer, state.output_size, Transform::Flipped180)?;
                frame.clear(Color32F::new(0.04, 0.05, 0.07, 1.0), &[damage])?;
                for (window_elements, title_bar_elements) in
                    window_elements.iter().zip(&title_bar_elements)
                {
                    draw_render_elements::<GlesRenderer, _, _>(
                        &mut frame,
                        state.viewport_scale,
                        window_elements,
                        &[damage],
                    )?;
                    draw_render_elements::<GlesRenderer, _, _>(
                        &mut frame,
                        1.0,
                        title_bar_elements,
                        &[damage],
                    )?;
                }
                draw_render_elements::<GlesRenderer, _, _>(
                    &mut frame,
                    1.0,
                    &state.control_bar_elements,
                    &[damage],
                )?;
                let _ = frame.finish()?;
            }

            for window in &state.windows {
                send_frames_surface_tree(
                    window.surface.wl_surface(),
                    start_time.elapsed().as_millis() as u32,
                );
            }

            display.flush_clients()?;
            backend.submit(Some(&[damage]))?;
            state.needs_redraw = false;
            if state.viewport_animation.is_some() {
                state.request_redraw();
            }
        } else {
            std::thread::sleep(IDLE_SLEEP);
        }
    }
}

fn configure_server_side_decoration(toplevel: &ToplevelSurface) {
    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ServerSide);
    });
    toplevel.send_configure();
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

            if let Some(drag) = state.drag.as_ref() {
                let delta = location - drag.pointer_start;
                if let Some(window) = state.windows.get_mut(drag.window_index) {
                    window.position = CanvasPoint {
                        x: drag.window_start.x + (delta.x / state.viewport_scale).round() as i32,
                        y: drag.window_start.y + (delta.y / state.viewport_scale).round() as i32,
                    };
                    state.request_redraw();
                }
                return;
            }

            let focus = match state.hit_test(location) {
                Some(HitTarget::Client {
                    surface,
                    surface_location,
                    ..
                }) => Some((surface, surface_location)),
                _ => None,
            };
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
            let is_left_button = event.button() == Some(smithay::backend::input::MouseButton::Left);

            if is_left_button && event.state() == ButtonState::Released && state.drag.is_some() {
                state.drag = None;
                return;
            }

            if is_left_button && event.state() == ButtonState::Pressed {
                if let Some(action) = state.control_action_at(state.pointer_location) {
                    state.drag = None;
                    state.run_control_action(action);
                    return;
                }

                match state.hit_test(state.pointer_location) {
                    Some(HitTarget::TitleBar { window_index }) => {
                        let window_index = state.raise_window(window_index);
                        let surface = state.windows[window_index].surface.wl_surface().clone();
                        let keyboard = state.keyboard.clone();
                        keyboard.set_focus(state, Some(surface), Serial::from(0));
                        state.drag = Some(DragState {
                            window_index,
                            pointer_start: state.pointer_location,
                            window_start: state.windows[window_index].position,
                        });
                        state.request_redraw();
                        return;
                    }
                    Some(HitTarget::Client { window_index, .. }) => {
                        let window_index = state.raise_window(window_index);
                        let surface = state.windows[window_index].surface.wl_surface().clone();
                        let keyboard = state.keyboard.clone();
                        keyboard.set_focus(state, Some(surface), Serial::from(0));
                    }
                    None => {
                        let keyboard = state.keyboard.clone();
                        keyboard.set_focus(state, Option::<WlSurface>::None, Serial::from(0));
                    }
                }
            }

            let focus = match state.hit_test(state.pointer_location) {
                Some(HitTarget::Client {
                    surface,
                    surface_location,
                    ..
                }) => Some((surface, surface_location)),
                _ => None,
            };

            if let Some((surface, _)) = focus.clone() {
                if event.state() == ButtonState::Pressed {
                    let keyboard = state.keyboard.clone();
                    keyboard.set_focus(state, Some(surface), Serial::from(0));
                }
            } else if is_left_button && event.state() == ButtonState::Pressed {
                return;
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
        window_index: usize,
    ) -> Vec<WaylandSurfaceRenderElement<GlesRenderer>> {
        let window = &self.windows[window_index];
        render_elements_from_surface_tree(
            renderer,
            window.surface.wl_surface(),
            self.content_screen_origin(window_index),
            self.viewport_scale,
            1.0,
            Kind::Unspecified,
        )
    }

    fn title_bar_elements(&self, window_index: usize) -> Vec<SolidColorRenderElement> {
        let mut elements = Vec::new();

        let rect = self.title_bar_rect(window_index);
        let focused_color = Color32F::new(0.19, 0.32, 0.55, 1.0);
        let unfocused_color = Color32F::new(0.15, 0.18, 0.24, 1.0);
        elements.push(solid_element(
            rect,
            if window_index == self.windows.len().saturating_sub(1) {
                focused_color
            } else {
                unfocused_color
            },
        ));

        let grip_color = Color32F::new(0.74, 0.82, 0.95, 1.0);
        let grip_margin = (12.0 * self.viewport_scale).round().max(4.0) as i32;
        let grip_height = (2.0 * self.viewport_scale).round().max(1.0) as i32;
        let grip_top = (8.0 * self.viewport_scale).round().max(3.0) as i32;
        let grip_gap = (6.0 * self.viewport_scale).round().max(3.0) as i32;
        for grip_index in 0..3 {
            elements.push(solid_element(
                Rectangle::new(
                    (
                        rect.loc.x + grip_margin,
                        rect.loc.y + grip_top + grip_index * grip_gap,
                    )
                        .into(),
                    ((rect.size.w - grip_margin * 2).max(1), grip_height).into(),
                ),
                grip_color,
            ));
        }

        elements
    }

    fn rebuild_control_bar_elements(&mut self) {
        let buttons = self.control_buttons();
        let mut elements = Vec::new();

        // draw_render_elements expects top-to-bottom ordering for occlusion.
        for button in &buttons {
            for label_rect in label_rects(*button) {
                elements.push(solid_element(
                    label_rect,
                    Color32F::new(0.92, 0.96, 1.0, 1.0),
                ));
            }
        }

        for button in buttons {
            elements.push(solid_element(
                button.rect,
                Color32F::new(0.22, 0.28, 0.38, 1.0),
            ));
        }

        elements.push(solid_element(
            Rectangle::new(
                (0, 0).into(),
                (self.output_size.w, CONTROL_BAR_HEIGHT).into(),
            ),
            Color32F::new(0.10, 0.12, 0.16, 0.96),
        ));

        self.control_bar_elements = elements;
    }

    fn control_buttons(&self) -> Vec<ControlButton> {
        default_control_buttons()
    }

    fn control_action_at(&self, point: Point<f64, Logical>) -> Option<ControlAction> {
        self.control_buttons()
            .into_iter()
            .find(|button| rect_contains(button.rect, point))
            .map(|button| button.action)
    }

    fn run_control_action(&mut self, action: ControlAction) {
        self.advance_viewport_animation();

        match action {
            ControlAction::SpawnApp => self.spawn_app(),
            ControlAction::PanLeft => self.pan_viewport_by(-self.horizontal_pan_step(), 0),
            ControlAction::PanRight => self.pan_viewport_by(self.horizontal_pan_step(), 0),
            ControlAction::PanUp => self.pan_viewport_by(0, -self.vertical_pan_step()),
            ControlAction::PanDown => self.pan_viewport_by(0, self.vertical_pan_step()),
            ControlAction::ZoomIn => self.animate_zoom_around_viewport_center(ZOOM_STEP),
            ControlAction::ZoomOut => self.animate_zoom_around_viewport_center(1.0 / ZOOM_STEP),
        }
        self.request_redraw();
    }

    fn spawn_app(&mut self) {
        self.next_spawn_position = CanvasPoint {
            x: self.viewport_offset.x
                + (f64::from(self.output_size.w) / 2.0 / self.viewport_scale).round() as i32
                - MIN_WINDOW_WIDTH / 2
                + self.spawn_offset,
            y: self.viewport_offset.y
                + (f64::from(self.output_size.h) / 2.0 / self.viewport_scale).round() as i32
                - 180
                + self.spawn_offset,
        };
        self.spawn_offset = (self.spawn_offset + SPAWN_OFFSET_STEP) % SPAWN_OFFSET_WRAP;

        match Command::new(DEFAULT_APP)
            .env("WAYLAND_DISPLAY", WAYLAND_DISPLAY_NAME)
            .spawn()
        {
            Ok(_) => {}
            Err(error) => eprintln!("Failed to spawn {DEFAULT_APP}: {error}"),
        }
    }

    fn hit_test(&self, location: Point<f64, Logical>) -> Option<HitTarget> {
        if location.y < f64::from(CONTROL_BAR_HEIGHT) {
            return None;
        }

        let canvas_location = self.screen_to_canvas(location);

        for (window_index, window) in self.windows.iter().enumerate().rev() {
            if rect_contains(self.title_bar_canvas_rect(window_index), canvas_location) {
                return Some(HitTarget::TitleBar { window_index });
            }

            let content_origin = self.content_canvas_origin(window_index);
            if let Some((surface, surface_location)) = under_from_surface_tree(
                window.surface.wl_surface(),
                canvas_location,
                content_origin,
                WindowSurfaceType::ALL,
            ) {
                let relative_surface_location = canvas_location - surface_location.to_f64();
                let pointer_focus_origin = location - relative_surface_location;
                return Some(HitTarget::Client {
                    window_index,
                    surface,
                    surface_location: pointer_focus_origin,
                });
            }
        }

        None
    }

    fn raise_window(&mut self, window_index: usize) -> usize {
        if window_index + 1 == self.windows.len() {
            return window_index;
        }

        let window = self.windows.remove(window_index);
        self.windows.push(window);
        self.request_redraw();
        self.windows.len() - 1
    }

    fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    fn start_viewport_animation(&mut self, to_offset: CanvasPoint, to_scale: f64) {
        if self.viewport_offset == to_offset
            && (self.viewport_scale - to_scale).abs() < f64::EPSILON
        {
            self.viewport_animation = None;
            return;
        }

        self.viewport_animation = Some(ViewportAnimation {
            from_offset: self.viewport_offset,
            from_scale: self.viewport_scale,
            to_offset,
            to_scale,
            started_at: Instant::now(),
        });
        self.request_redraw();
    }

    fn advance_viewport_animation(&mut self) {
        let Some(animation) = &self.viewport_animation else {
            return;
        };

        let progress = (animation.started_at.elapsed().as_secs_f64()
            / VIEWPORT_ANIMATION_DURATION.as_secs_f64())
        .clamp(0.0, 1.0);
        let eased = ease_out_cubic(progress);

        self.viewport_offset =
            interpolate_canvas_point(animation.from_offset, animation.to_offset, eased);
        self.viewport_scale = interpolate_f64(animation.from_scale, animation.to_scale, eased);

        if progress >= 1.0 {
            let animation = self.viewport_animation.take().unwrap();
            self.viewport_offset = animation.to_offset;
            self.viewport_scale = animation.to_scale;
        } else {
            self.request_redraw();
        }
    }

    fn pan_viewport_by(&mut self, x: i32, y: i32) {
        self.start_viewport_animation(
            CanvasPoint {
                x: self.viewport_offset.x + x,
                y: self.viewport_offset.y + y,
            },
            self.viewport_scale,
        );
    }

    fn horizontal_pan_step(&self) -> i32 {
        (f64::from(self.output_size.w) / 2.0 / self.viewport_scale).round() as i32
    }

    fn vertical_pan_step(&self) -> i32 {
        (f64::from(self.output_size.h) / 2.0 / self.viewport_scale).round() as i32
    }

    fn title_bar_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let canvas_rect = self.title_bar_canvas_rect(window_index);
        let origin = self
            .canvas_to_screen(canvas_rect.loc.to_f64())
            .to_i32_round();
        Rectangle::new(
            origin,
            (
                (f64::from(canvas_rect.size.w) * self.viewport_scale).round() as i32,
                (f64::from(canvas_rect.size.h) * self.viewport_scale).round() as i32,
            )
                .into(),
        )
    }

    fn title_bar_canvas_rect(&self, window_index: usize) -> Rectangle<i32, Logical> {
        let window = &self.windows[window_index];
        let content_bbox = bbox_from_surface_tree(
            window.surface.wl_surface(),
            self.content_canvas_origin(window_index),
        );
        Rectangle::new(
            (window.position.x, window.position.y).into(),
            (content_bbox.size.w.max(MIN_WINDOW_WIDTH), TITLE_BAR_HEIGHT).into(),
        )
    }

    fn content_canvas_origin(&self, window_index: usize) -> Point<i32, Logical> {
        let window = &self.windows[window_index];
        Point::<i32, Logical>::from((window.position.x, window.position.y + TITLE_BAR_HEIGHT))
    }

    fn content_screen_origin(&self, window_index: usize) -> Point<i32, Physical> {
        self.canvas_to_screen(self.content_canvas_origin(window_index).to_f64())
            .to_i32_round()
            .to_physical(1)
    }

    fn canvas_to_screen(&self, point: Point<f64, Logical>) -> Point<f64, Logical> {
        transform_canvas_to_screen(point, self.viewport_offset, self.viewport_scale)
    }

    fn screen_to_canvas(&self, point: Point<f64, Logical>) -> Point<f64, Logical> {
        transform_screen_to_canvas(point, self.viewport_offset, self.viewport_scale)
    }

    fn animate_zoom_around_viewport_center(&mut self, multiplier: f64) {
        let center_screen = Point::<f64, Logical>::from((
            f64::from(self.output_size.w) / 2.0,
            f64::from(self.output_size.h) / 2.0,
        ));
        let (viewport_offset, viewport_scale) = zoom_around_screen_point(
            self.viewport_offset,
            self.viewport_scale,
            center_screen,
            multiplier,
        );
        self.start_viewport_animation(viewport_offset, viewport_scale);
    }
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
delegate_output!(App);
delegate_xdg_decoration!(App);
