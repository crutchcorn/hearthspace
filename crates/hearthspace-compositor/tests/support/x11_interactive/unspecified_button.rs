//! Chromium/Electron omit the EWMH button number during pointer operations.
use super::*;

fn unspecified_button_operation(direction: u32, expected: [i32; 4]) {
    let (compositor, mut client) = fixture();
    let point = press(&compositor, &mut client);
    // Electron waits for its drag threshold, then sends button zero even
    // though the actual input sequence is a live primary-button press.
    motion(&compositor, i32::from(point.0) + 20, i32::from(point.1) + 8);
    client.request(&compositor, direction, 0, point);
    client.clear_events();
    motion(
        &compositor,
        i32::from(point.0) + 80,
        i32::from(point.1) + 50,
    );
    wait_geometry(&compositor, &mut client, expected, RED);
    assert_eq!(client.motions, 0, "operation motion leaked to the client");
    action(&compositor, "pointer-button-up 272");
    motion(&compositor, 600, 500);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, expected, RED);
    responsive_click(&compositor, &mut client);
}

#[test]
fn x11_unspecified_button_csd_move_uses_live_primary_grab() {
    let _guard = headless_test_guard();
    unspecified_button_operation(8, [180, 200, 320, 240]);
}

#[test]
fn x11_unspecified_button_csd_resize_uses_live_primary_grab() {
    let _guard = headless_test_guard();
    unspecified_button_operation(4, [100, 150, 400, 290]);
}

fn reject_without_primary_grab(direction: u32) {
    let (compositor, mut client) = fixture();
    // No press/serial has ever been delivered to this client.
    client.request(&compositor, direction, 0, (160, 170));
    motion(&compositor, 180, 180);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [100, 150, 320, 240], RED);

    // A recent serial from a released left-button sequence is insufficient.
    let point = press(&compositor, &mut client);
    action(&compositor, "pointer-button-up 272");
    client.wait_release();
    client.request(&compositor, direction, 0, point);
    motion(&compositor, 200, 200);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [100, 150, 320, 240], RED);

    // An unspecified button must not authorize an actually held right button.
    motion(&compositor, 160, 170);
    client.clear_events();
    action(&compositor, "pointer-button-down 273");
    let event = client.wait_press();
    assert_eq!(event.detail, 3);
    client.request(&compositor, direction, 0, (160, 170));
    motion(&compositor, 220, 200);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [100, 150, 320, 240], RED);
    action(&compositor, "pointer-button-up 273");
    client.wait_release();
    assert_eq!(client.presses.len(), 1);
    assert_eq!(client.releases.len(), 1);
    assert_eq!(client.releases[0].detail, 3);
    responsive_click(&compositor, &mut client);
}

#[test]
fn x11_unspecified_button_csd_move_rejects_missing_released_and_right_grabs() {
    let _guard = headless_test_guard();
    reject_without_primary_grab(8);
}

#[test]
fn x11_unspecified_button_csd_resize_rejects_missing_released_and_right_grabs() {
    let _guard = headless_test_guard();
    reject_without_primary_grab(4);
}
