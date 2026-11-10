//! Stale cancellation cannot acquire authority over a newer operation.
mod fixture;
use fixture::Fixture;

#[test]
fn cancellation_is_hidden_from_ordinary_wayland_clients() {
    Fixture::new().ordinary_client_cannot_see_manager();
}

#[test]
fn cancellation_stops_geometry_but_preserves_the_draining_pointer_grab() {
    let mut f = Fixture::new();
    let serial = f.start_move();
    f.cancel(&f.top.clone(), serial, 0);
    assert!(f.app.drag.is_none());
    assert!(f.app.has_window_pointer_grab());
    assert!(f.app.pressed_pointer_buttons.contains(&0x110));
    f.cancel(&f.top.clone(), serial, 0); // duplicate is inert
    assert!(f.app.has_window_pointer_grab());
    f.release();
    assert!(!f.app.has_window_pointer_grab());
}

#[test]
fn stale_serial_cannot_cancel_a_new_move_on_the_same_toplevel() {
    let mut f = Fixture::new();
    let previous = f.start_move();
    f.release();
    let current = f.start_move();
    assert_ne!(previous, current);
    f.cancel(&f.top.clone(), previous, 0);
    assert!(f.app.drag.is_some());
    f.cancel(&f.top.clone(), current, 0);
    assert!(f.app.drag.is_none());
    f.release();
}

#[test]
fn wrong_toplevel_or_seat_cannot_cancel_the_current_move() {
    let mut f = Fixture::new();
    let other = f.other_toplevel();
    let serial = f.start_move();
    f.cancel(&other, serial, 0);
    assert!(f.app.drag.is_some());
    f.cancel(&f.top.clone(), serial, 1);
    assert!(f.app.drag.is_some());
    f.cancel(&f.top.clone(), serial, 0);
    assert!(f.app.drag.is_none());
    f.release();
}

#[test]
fn completed_operation_cannot_cancel_a_later_compositor_owned_move() {
    let mut f = Fixture::new();
    let serial = f.start_move();
    f.release();
    f.app.begin_window_move_at(f.index, (10., 10.).into());
    f.cancel(&f.top.clone(), serial, 0);
    assert!(
        f.app.drag.is_some(),
        "no matching input grab authorizes this cancellation"
    );
}
