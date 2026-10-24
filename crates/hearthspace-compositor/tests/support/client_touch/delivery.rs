//! Native scrolling input must never become a text-selection pointer drag.
use super::*;
use wayland_client::protocol::wl_pointer::{ButtonState, Event as PointerEvent};

fn native_drag(scale: f64, inset: (i32, i32)) {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture(inset, scale);
    client.enable_touch();
    let viewport = query(&compositor, "status")["viewport"].clone();
    let surface_point =
        |point: (f64, f64)| (point.0 + f64::from(inset.0), point.1 + f64::from(inset.1));
    let start = (40.25, 30.5);
    down(&compositor, 0, screen(&compositor, start));
    frame(&compositor);
    let serial = assert_down(&mut client, 0, surface_point(start));

    let moved = (80.75, 60.25);
    motion(&compositor, 0, screen(&compositor, moved));
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [Event::Motion { id: 0, x, y, .. }, Event::Frame]
            if (*x, *y) == surface_point(moved)),
        "drag must deliver exact unscaled surface coordinates: {events:?}",
    );

    let end = (120.5, 100.75);
    motion(&compositor, 0, screen(&compositor, end));
    // Real devices can report final motion and lift together in one frame.
    up(&compositor, 0);
    frame(&compositor);
    let events = client.take_touch_events();
    assert!(
        matches!(events.as_slice(), [
            Event::Motion { id: 0, x, y, .. }, Event::Up { id: 0, .. }, Event::Frame,
        ] if (*x, *y) == surface_point(end)),
        "final motion must precede one native up and frame: {events:?}",
    );
    assert_no_pointer_buttons(&mut client);
    for field in ["scale", "x", "y"] {
        assert_eq!(
            query(&compositor, "status")["viewport"][field],
            viewport[field],
            "one-finger client drag must not navigate the canvas",
        );
    }

    // Reuse the slot twice: a finished contact must not suppress later taps.
    let next = fresh_tap(&compositor, &mut client, inset);
    assert_ne!(next, serial, "new down needs its own authority serial");
    assert_ne!(fresh_tap(&compositor, &mut client, inset), next);
}

#[test]
fn native_drag_at_canvas_scale_one() {
    native_drag(1.0, (0, 0));
}

#[test]
fn native_drag_at_canvas_scale_half() {
    native_drag(0.5, (0, 0));
}

#[test]
fn native_drag_with_geometry_inset_at_canvas_scale_one() {
    native_drag(1.0, (12, 8));
}

#[test]
fn native_drag_with_geometry_inset_at_canvas_scale_half() {
    native_drag(0.5, (12, 8));
}

#[test]
fn pointer_only_client_keeps_drag_and_tap_fallback() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture((0, 0), 1.0);
    // Deliberately do not enable_touch: a seat alone does not opt into wl_touch.
    down(&compositor, 0, screen(&compositor, (40.0, 30.0)));
    frame(&compositor);
    motion(&compositor, 0, screen(&compositor, (80.0, 60.0)));
    frame(&compositor);
    let events = client.take_pointer_events();
    assert_buttons(&events, &[(272, ButtonState::Pressed)]);
    assert!(
        events.iter().any(|event| matches!(event,
            PointerEvent::Motion { surface_x, surface_y, .. }
                if (*surface_x, *surface_y) == (80.0, 60.0))),
        "pointer-only drag must reach the client: {events:?}",
    );
    up(&compositor, 0);
    frame(&compositor);
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Released)],
    );
    down(&compositor, 0, screen(&compositor, (60.0, 44.0)));
    up(&compositor, 0);
    frame(&compositor);
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Pressed), (272, ButtonState::Released)],
    );
    assert!(client.take_touch_events().is_empty());
}
