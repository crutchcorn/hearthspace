//! Client wheel delivery and move/resize ownership must survive canvas zoom routing.
use super::*;
use wayland_client::{WEnum, protocol::wl_pointer::Axis};

#[rstest::rstest]
#[case::ordinary(false)]
#[case::ctrl(true)]
fn ordinary_and_ctrl_wheel_reach_client_without_canvas_zoom(#[case] ctrl: bool) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    press(&compositor, &mut client, 0);
    action(&compositor, "pointer-button-up 272");
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Released)],
    );
    client.take_keyboard_events();
    let viewport = query(&compositor, "status")["viewport"].clone();

    if ctrl {
        action(&compositor, "key-down 29");
    }
    action(&compositor, "axis 11 -120");
    let events = client.take_pointer_events();
    let axes: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::Axis {
                axis: WEnum::Value(axis),
                value,
                ..
            } => Some((*axis, *value)),
            _ => None,
        })
        .collect();
    assert_eq!(
        axes.len(),
        2,
        "wheel input did not reach the client: {events:?}"
    );
    assert!(axes.contains(&(Axis::HorizontalScroll, 11.0)));
    assert!(axes.contains(&(Axis::VerticalScroll, -120.0)));
    assert_buttons(&events, &[]);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    if ctrl {
        action(&compositor, "key-up 29");
        assert_keys(&mut client, 29);
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[rstest::rstest]
#[case::move_window(Operation::Move)]
#[case::resize_window(Operation::Resize)]
fn super_wheel_preserves_window_operation_and_draining_button_ownership(
    #[case] operation: Operation,
) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let viewport = query(&compositor, "status")["viewport"].clone();
    let original = geometry(&window(&compositor, 0));
    let (point, serial) = press(&compositor, &mut client, 0);
    operation.request(&mut client, 0, serial);
    assert_leave(&mut client, 0);
    motion(&compositor, (point.0 + 20.0, point.1 + 10.0));
    assert_silent(&mut client, "operation leaked motion");
    let moved = operation.expected(original, 20, 10);
    assert_eq!(geometry(&window(&compositor, 0)), moved);
    action(&compositor, "pointer-button-down 273");
    action(&compositor, "key-down 125");
    action(&compositor, "axis 0 -120");
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    assert_eq!(geometry(&window(&compositor, 0)), moved);
    assert_silent(&mut client, "owned scroll reached the client");

    // Finishing geometry still leaves the secondary button owned. Its scroll
    // must not start navigation or transfer an unmatched release to a client.
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "axis 0 120");
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    assert_eq!(geometry(&window(&compositor, 0)), moved);
    assert_silent(&mut client, "scroll escaped while a button was draining");
    action(&compositor, "key-up 125");
    action(&compositor, "pointer-button-up 273");
    let events = client.take_pointer_events();
    assert_reentered(&client, &events, 0);
    assert_content_click(&compositor, &mut client);

    action(&compositor, "key-down 125");
    action(&compositor, "axis 0 -120");
    action(&compositor, "key-up 125");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let after = query(&compositor, "status")["viewport"].clone();
        if after["scale"].as_f64().unwrap() > viewport["scale"].as_f64().unwrap() {
            break;
        }
        assert!(Instant::now() < deadline, "released operation blocked zoom");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(geometry(&window(&compositor, 0)), moved);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
