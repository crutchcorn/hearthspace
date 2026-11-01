//! Smithay handles protocol objects; this fixture never opens a renderer/device.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use smithay::{
    backend::input::{ButtonState, InputTime, KeyState},
    input::{
        Seat, SeatHandler, SeatState,
        keyboard::{FilterResult, KeyboardHandle, XkbConfig},
        pointer::{
            ButtonEvent, CursorImageStatus, MotionEvent, PointerHandle, RelativeMotionEvent,
        },
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    utils::SERIAL_COUNTER,
    wayland::{
        buffer::BufferHandler,
        compositor::{
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
            SurfaceAttributes, with_states,
        },
        output::OutputHandler,
        pointer_constraints::{
            PointerConstraint, PointerConstraintsHandler, PointerConstraintsState,
            with_pointer_constraint,
        },
        relative_pointer::RelativePointerManagerState,
        shm::{ShmHandler, ShmState},
    },
};
use std::collections::HashMap;
use wayland_server::{
    Client, DisplayHandle, Resource,
    backend::ClientData,
    protocol::{wl_buffer::WlBuffer, wl_surface::WlSurface},
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Snapshot {
    pub surfaces: Vec<u32>,
    pub accepted_locks: usize,
    pub locked_on: Option<u32>,
    pub cursor_hidden: bool,
    pub cursor_surface: Option<u32>,
    pub relative_frames: Vec<f64>,
    pub cursor_hints: Vec<[f64; 2]>,
    pub clicks: usize,
}

pub enum Input {
    Inspect,
    Focus(u32, f64, f64),
    Relative(f64),
    Click,
    Fence,
    Stop,
}

pub struct State {
    compositor: CompositorState,
    shm: ShmState,
    seats: SeatState<Self>,
    pointer: PointerHandle<Self>,
    keyboard: KeyboardHandle<Self>,
    surfaces: HashMap<u32, WlSurface>,
    output: Output,
    stats: Snapshot,
}

impl State {
    pub fn new(dh: &DisplayHandle) -> Result<Self> {
        RelativePointerManagerState::new::<Self>(dh);
        PointerConstraintsState::new::<Self>(dh);
        let mut seats = SeatState::new();
        let mut seat = seats.new_wl_seat(dh, "owned-relative-regression");
        let pointer = seat.add_pointer();
        let keyboard = seat.add_keyboard(
            XkbConfig {
                rules: "evdev",
                model: "pc105",
                layout: "us",
                variant: "",
                options: Some(String::new()),
            },
            200,
            25,
        )?;
        let output = Output::new(
            "owned-headless".into(),
            PhysicalProperties {
                size: (320, 200).into(),
                subpixel: Subpixel::Unknown,
                make: "Fixture".into(),
                model: "No hardware".into(),
                serial_number: "private".into(),
            },
        );
        let mode = Mode {
            size: (1280, 800).into(),
            refresh: 60_000,
        };
        output.change_current_state(Some(mode), None, None, Some((0, 0).into()));
        output.set_preferred(mode);
        output.create_global::<Self>(dh);
        Ok(Self {
            compositor: CompositorState::new::<Self>(dh),
            shm: ShmState::new::<Self>(dh, []),
            seats,
            pointer,
            keyboard,
            surfaces: HashMap::new(),
            output,
            stats: Snapshot::default(),
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut stats = self.stats.clone();
        stats.surfaces = self.surfaces.keys().copied().collect();
        stats.surfaces.sort_unstable();
        stats.locked_on = self.surfaces.iter().find_map(|(id, surface)| {
            with_pointer_constraint(surface, &self.pointer, |constraint| {
                constraint
                    .filter(|c| matches!(&**c, PointerConstraint::Locked(_)) && c.is_active())
                    .map(|_| *id)
            })
        });
        stats
    }

    pub fn input(&mut self, input: Input) -> Result<()> {
        let pointer = self.pointer.clone();
        match input {
            Input::Inspect | Input::Stop => {}
            Input::Focus(id, x, y) => {
                ensure!(
                    self.snapshot().locked_on.is_none(),
                    "must release lock before focus change"
                );
                let surface = self
                    .surfaces
                    .get(&id)
                    .context("unknown owned wl_surface")?
                    .clone();
                self.output.enter(&surface);
                pointer.motion(
                    self,
                    Some((surface.clone(), (0., 0.).into())),
                    &MotionEvent {
                        location: (x, y).into(),
                        serial: SERIAL_COUNTER.next_serial(),
                        time: InputTime::now(),
                    },
                );
                pointer.frame(self);
                let keyboard = self.keyboard.clone();
                keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
            }
            Input::Relative(dx) => {
                let focus = pointer
                    .current_focus()
                    .context("relative motion has no focus")?;
                ensure!(
                    self.snapshot().locked_on == Some(focus.id().protocol_id()),
                    "relative path requires accepted lock on focus"
                );
                // Deliberately NO pointer.motion: only the actual relative protocol
                // callback and a wl_pointer.frame may carry these three deltas.
                pointer.relative_motion(
                    self,
                    Some((focus, (0., 0.).into())),
                    &RelativeMotionEvent {
                        delta: (dx, 0.).into(),
                        delta_unaccel: (dx, 0.).into(),
                        time: InputTime::now(),
                    },
                );
                pointer.frame(self);
                self.stats.relative_frames.push(dx);
                self.fence();
            }
            Input::Click => {
                for state in [ButtonState::Pressed, ButtonState::Released] {
                    pointer.button(
                        self,
                        &ButtonEvent {
                            serial: SERIAL_COUNTER.next_serial(),
                            time: InputTime::now(),
                            button: 272,
                            state,
                        },
                    );
                    pointer.frame(self);
                }
                self.stats.clicks += 1;
                self.fence();
            }
            Input::Fence => self.fence(),
        }
        Ok(())
    }

    fn fence(&mut self) {
        // Same Wayland connection, after the pointer frame. An explicit async
        // X keyboard grab routes this balanced marker to the observer.
        let keyboard = self.keyboard.clone();
        for state in [KeyState::Pressed, KeyState::Released] {
            keyboard.input::<(), _>(
                self,
                38u32.into(),
                state,
                SERIAL_COUNTER.next_serial(),
                InputTime::now(),
                |_, _, _| FilterResult::Forward,
            );
        }
    }
}

#[derive(Default)]
pub struct Data(pub CompositorClientState);
impl ClientData for Data {}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }
    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<Data>().unwrap().0
    }
    fn new_surface(&mut self, surface: &WlSurface) {
        self.surfaces
            .insert(surface.id().protocol_id(), surface.clone());
    }
    fn commit(&mut self, surface: &WlSurface) {
        // This test consumes no pixels. Release SHM buffers and complete frame
        // callbacks so Xwayland can continue committing cursor-position hints.
        with_states(surface, |states| {
            let mut attributes = states.cached_state.get::<SurfaceAttributes>();
            let current = attributes.current();
            if let Some(BufferAssignment::NewBuffer(buffer)) = current.buffer.take() {
                buffer.release();
            }
            for callback in current.frame_callbacks.drain(..) {
                callback.done(0);
            }
            current.damage.clear();
        });
    }
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;
    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seats
    }
    fn cursor_image(&mut self, _: &Seat<Self>, image: CursorImageStatus) {
        self.stats.cursor_hidden = image == CursorImageStatus::Hidden;
        self.stats.cursor_surface = match image {
            CursorImageStatus::Surface(surface) => Some(surface.id().protocol_id()),
            _ => None,
        };
    }
}

impl PointerConstraintsHandler for State {
    fn cursor_position_hint(
        &mut self,
        _: &WlSurface,
        _: &PointerHandle<Self>,
        location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) {
        // Evidence only. Applying hints back to input would mask producer drift.
        if self.stats.cursor_hints.len() < 64 {
            self.stats.cursor_hints.push([location.x, location.y]);
        }
    }

    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        if pointer.current_focus().as_ref() != Some(surface) {
            return;
        }
        let locked = with_pointer_constraint(surface, pointer, |constraint| {
            let constraint = constraint.expect("new constraint must exist");
            constraint.activate(); // Sends real zwp_locked_pointer_v1.locked.
            matches!(&*constraint, PointerConstraint::Locked(_))
        });
        if locked {
            self.stats.accepted_locks += 1;
        }
    }
}
impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}
impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _: &WlBuffer) {}
}
impl OutputHandler for State {}
smithay::delegate_dispatch2!(State);
