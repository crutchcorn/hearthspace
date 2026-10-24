//! Smithay cancellation across frame boundaries, plus real shell-client delivery.
use super::{navigation::action, shell_panels::connect, *};
use smithay::{
    backend::input::InputTime,
    input::{
        Seat, SeatHandler, SeatState,
        touch::{
            DownEvent, FrameMarker, MotionEvent, OrientationEvent, ShapeEvent, TouchHandle,
            TouchTarget, UpEvent,
        },
    },
    utils::{IsAlive, SERIAL_COUNTER},
    wayland::pointer_constraints::PointerConstraintsHandler,
};
use wayland_server::protocol::wl_surface::WlSurface;

#[test]
fn touch_cancel_after_frame_reaches_shell_client_and_allows_fresh_touch() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    client.enable_touch();
    let dialog = client.add(
        hearthspace_common::POWER_CONFIRM_APP_ID.into(),
        (420, 220),
        (0, 0),
        [0x66, 0xcc, 0, 0xff],
    );
    action(&compositor, "touch-down cancel-test 0 450 310");
    action(&compositor, "touch-frame cancel-test");
    assert_eq!(client.touches(), &[(dialog, (20., 20.))]);
    action(&compositor, "touch-cancel cancel-test");
    assert_eq!(client.touch_cancellations(), 1);
    action(&compositor, "touch-cancel cancel-test");
    action(&compositor, "touch-frame cancel-test");
    assert_eq!(client.touch_cancellations(), 1);

    action(&compositor, "touch-down cancel-test 0 460 320");
    action(&compositor, "touch-frame cancel-test");
    action(&compositor, "touch-up cancel-test 0");
    action(&compositor, "touch-frame cancel-test");
    assert_eq!(client.touches().len(), 2);
    assert_eq!(client.touches().last(), Some(&(dialog, (30., 30.))));
    assert_eq!(client.touch_cancellations(), 1);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

// Distinct targets can share a client's marker, just as WlSurface does. The real
// TouchHandle owns slots, frame markers and implicit grabs in these tests.
#[derive(Debug, Clone, PartialEq)]
struct Target {
    client: usize,
    surface: usize,
}

#[derive(Default)]
struct Observations {
    down: usize,
    up: usize,
    motion: usize,
    frames: usize,
    cancellations: usize,
    marker: Option<FrameMarker>,
}

#[derive(Default)]
struct State {
    seats: SeatState<Self>,
    clients: [Observations; 2],
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = Target;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seats
    }
}

impl PointerConstraintsHandler for State {}

impl IsAlive for Target {
    fn alive(&self) -> bool {
        true
    }
}

impl TouchTarget<State> for Target {
    fn down(&self, _: &Seat<State>, data: &mut State, _: &DownEvent) {
        data.clients[self.client].down += 1;
    }

    fn up(&self, _: &Seat<State>, data: &mut State, _: &UpEvent) {
        data.clients[self.client].up += 1;
    }

    fn motion(&self, _: &Seat<State>, data: &mut State, _: &MotionEvent) {
        data.clients[self.client].motion += 1;
    }

    fn frame(&self, _: &Seat<State>, data: &mut State, marker: FrameMarker) {
        data.clients[self.client].frames += 1;
        data.clients[self.client].marker = Some(marker);
    }

    fn cancel(&self, _: &Seat<State>, data: &mut State, marker: FrameMarker) {
        data.clients[self.client].cancellations += 1;
        data.clients[self.client].marker = Some(marker);
    }

    fn shape(&self, _: &Seat<State>, _: &mut State, _: &ShapeEvent) {}

    fn orientation(&self, _: &Seat<State>, _: &mut State, _: &OrientationEvent) {}

    fn last_frame(&self, _: &Seat<State>, data: &mut State) -> Option<FrameMarker> {
        data.clients[self.client].marker
    }
}

fn seat() -> (State, TouchHandle<State>) {
    let mut state = State::default();
    let touch = state.seats.new_seat("touch-cancel").add_touch();
    (state, touch)
}

fn down(touch: &TouchHandle<State>, state: &mut State, slot: u32, client: usize) {
    touch.down(
        state,
        Some((
            Target {
                client,
                surface: slot as usize,
            },
            (0., 0.).into(),
        )),
        &DownEvent {
            slot: Some(slot).into(),
            location: (20., 20.).into(),
            serial: SERIAL_COUNTER.next_serial(),
            time: InputTime::now(),
        },
    );
}

fn up(touch: &TouchHandle<State>, state: &mut State, slot: u32) {
    touch.up(
        state,
        &UpEvent {
            slot: Some(slot).into(),
            serial: SERIAL_COUNTER.next_serial(),
            time: InputTime::now(),
        },
    );
}

fn motion(touch: &TouchHandle<State>, state: &mut State, slot: u32) {
    touch.motion(
        state,
        None,
        &MotionEvent {
            slot: Some(slot).into(),
            location: (30., 30.).into(),
            time: InputTime::now(),
        },
    );
}

#[test]
fn touch_cancel_deduplicates_contacts_and_surfaces_on_one_client_after_frame() {
    let (mut state, touch) = seat();
    down(&touch, &mut state, 0, 0);
    down(&touch, &mut state, 1, 0);
    // Release the focus grab without ending contacts, allowing another surface.
    touch.unset_grab(&mut state);
    down(&touch, &mut state, 2, 0);
    touch.frame(&mut state);
    assert_eq!(state.clients[0].down, 3);
    assert_eq!(state.clients[0].frames, 1);
    assert!(touch.is_grabbed());
    touch.cancel(&mut state);
    assert_eq!(state.clients[0].cancellations, 1);
    assert!(!touch.is_grabbed());
    motion(&touch, &mut state, 0);
    motion(&touch, &mut state, 1);
    motion(&touch, &mut state, 2);
    touch.frame(&mut state);
    assert_eq!(state.clients[0].motion, 0);
    assert_eq!(state.clients[0].frames, 1);
    touch.cancel(&mut state);
    assert_eq!(state.clients[0].cancellations, 1);
}

#[test]
fn touch_cancel_reaches_unchanged_and_new_contacts_on_different_clients() {
    let (mut state, touch) = seat();
    down(&touch, &mut state, 0, 0);
    touch.frame(&mut state);
    touch.unset_grab(&mut state);
    down(&touch, &mut state, 1, 1);
    // Client 0 has no event in the pending frame belonging to client 1.
    touch.cancel(&mut state);
    assert_eq!(state.clients[0].cancellations, 1);
    assert_eq!(state.clients[1].cancellations, 1);
    assert!(!touch.is_grabbed());
    motion(&touch, &mut state, 0);
    motion(&touch, &mut state, 1);
    touch.frame(&mut state);
    assert_eq!(state.clients[0].motion, 0);
    assert_eq!(state.clients[1].motion, 0);
    assert_eq!(state.clients[0].frames, 1);
    assert_eq!(state.clients[1].frames, 0);
}

#[test]
fn touch_cancel_discards_pending_up_frames() {
    let (mut state, touch) = seat();
    down(&touch, &mut state, 0, 0);
    down(&touch, &mut state, 1, 0);
    touch.frame(&mut state);
    up(&touch, &mut state, 0);
    touch.cancel(&mut state);
    assert_eq!(state.clients[0].cancellations, 1);
    // A late event for the released slot must not flush its old up-frame target.
    motion(&touch, &mut state, 0);
    touch.frame(&mut state);
    assert_eq!(state.clients[0].frames, 1);
    assert_eq!(state.clients[0].motion, 0);
    up(&touch, &mut state, 1);
    touch.frame(&mut state);
    assert_eq!(state.clients[0].up, 1);
    assert_eq!(state.clients[0].frames, 1);
}

#[test]
fn touch_cancel_without_active_contacts_is_idempotent_and_allows_slot_reuse() {
    let (mut state, touch) = seat();
    touch.cancel(&mut state);
    touch.cancel(&mut state);
    assert_eq!(state.clients[0].cancellations, 0);
    down(&touch, &mut state, 0, 0);
    touch.frame(&mut state);
    touch.cancel(&mut state);
    touch.cancel(&mut state);
    assert_eq!(state.clients[0].cancellations, 1);
    down(&touch, &mut state, 0, 0);
    touch.frame(&mut state);
    assert!(touch.is_grabbed());
    motion(&touch, &mut state, 0);
    up(&touch, &mut state, 0);
    touch.frame(&mut state);
    assert!(!touch.is_grabbed());
    touch.cancel(&mut state);
    assert_eq!(state.clients[0].down, 2);
    assert_eq!(state.clients[0].motion, 1);
    assert_eq!(state.clients[0].up, 1);
    assert_eq!(state.clients[0].frames, 3);
    assert_eq!(state.clients[0].cancellations, 1);
}
