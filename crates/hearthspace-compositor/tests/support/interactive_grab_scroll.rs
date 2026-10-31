//! Scroll must not change the transform owned by a held move/resize sequence.
use super::*;

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
