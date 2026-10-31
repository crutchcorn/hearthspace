//! Real X11 CSD requests, coordinates, cancellation, and subsequent input.
use super::{
    navigation::{action, query, wait_windows},
    *,
};

#[path = "x11_interactive/assertions.rs"]
mod assertions;
#[path = "x11_interactive/cancellation.rs"]
mod cancellation;
#[path = "x11_interactive/canvas_input.rs"]
mod canvas_input;
#[path = "x11_interactive/client.rs"]
mod client;
#[path = "frame_layout/x11_persistence.rs"]
mod frame_persistence;
#[path = "x11_interactive/identity.rs"]
mod identity;
#[path = "x11_interactive/presentation.rs"]
mod presentation;
#[path = "x11_interactive/touch.rs"]
mod touch;
use assertions::*;
use client::{Client, GREEN, RED};

fn fixture() -> (HeadlessCompositor, Client) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = Client::connect(&compositor);
    wait_geometry(&compositor, &mut client, [100, 150, 320, 240], RED);
    (compositor, client)
}

#[test]
fn x11_csd_move_synchronizes_root_coordinates_and_survives_unrelated_popups() {
    let _guard = headless_test_guard();
    let (compositor, mut client) = fixture();
    let unmapped_popup = client.overlay(&compositor);
    let destroyed_popup = client.overlay(&compositor);
    let point = press(&compositor, &mut client);
    // Toolkit drag thresholds can defer the request until after motion.
    motion(&compositor, point.screen.0 + 20, point.screen.1 + 8);
    client.request(&compositor, 8, 1, point.root);
    wait_geometry(&compositor, &mut client, [120, 158, 320, 240], RED);
    // Canvas-only moves skip an unchanged backing configure. The real X
    // origin stays fixed; a resize below still requires a synthetic notify.
    assert_eq!(
        client.origin(),
        (0, 0),
        "canvas movement must preserve the actual X root origin"
    );
    client.clear_events();
    client.unmap(&compositor, unmapped_popup);
    motion(&compositor, point.screen.0 + 60, point.screen.1 + 30);
    wait_geometry(&compositor, &mut client, [160, 180, 320, 240], RED);
    // A cancel addressed to another window must not terminate this drag.
    client.request_for(&compositor, destroyed_popup, 11, 0, (0, 0));
    client.destroy(&compositor, destroyed_popup);
    motion(&compositor, point.screen.0 + 100, point.screen.1 + 50);
    wait_geometry(&compositor, &mut client, [200, 200, 320, 240], RED);
    assert_eq!(
        client.motions, 0,
        "compositor drag motion leaked into the application"
    );
    action(&compositor, "pointer-button-up 272");
    motion(&compositor, 500, 400);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [200, 200, 320, 240], RED);
    responsive_click(&compositor, &mut client);
    let point = press(&compositor, &mut client);
    client.request(&compositor, 8, 1, point.root);
    motion(&compositor, point.screen.0 + 30, point.screen.1 + 20);
    action(&compositor, "pointer-button-up 272");
    wait_geometry(&compositor, &mut client, [230, 220, 320, 240], GREEN);
}

fn cancel_held_operation(direction: u32, expected: [i32; 4]) {
    let (compositor, mut client) = fixture();
    let point = press(&compositor, &mut client);
    client.request(&compositor, direction, 1, point.root);
    motion(&compositor, point.screen.0 + 80, point.screen.1 + 50);
    wait_geometry(&compositor, &mut client, expected, RED);
    if direction == 4 {
        let origin = client.origin();
        client.pump();
        assert!(
            client.configures.iter().any(|event| {
                event.response_type & 0x80 != 0
                    && (i32::from(event.x), i32::from(event.y)) == origin
                    && [i32::from(event.width), i32::from(event.height)]
                        == [expected[2], expected[3]]
            }),
            "resize must advertise actual root coordinates and size through synthetic ConfigureNotify: {:?}",
            client.configures
        );
    }
    client.request(&compositor, 11, 0, (0, 0));
    client.clear_events();
    motion(&compositor, point.screen.0 + 120, point.screen.1 + 80);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, expected, RED);
    assert_eq!(
        client.motions, 0,
        "canceled held sequence must remain captured"
    );
    action(&compositor, "pointer-button-up 272");
    motion(&compositor, 600, 500);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, expected, RED);
    responsive_click(&compositor, &mut client);
    let point = press(&compositor, &mut client);
    client.request(&compositor, 8, 1, point.root);
    motion(&compositor, point.screen.0 + 30, point.screen.1 + 20);
    action(&compositor, "pointer-button-up 272");
    wait_geometry(
        &compositor,
        &mut client,
        [expected[0] + 30, expected[1] + 20, expected[2], expected[3]],
        GREEN,
    );
}

#[test]
fn x11_csd_move_cancel_while_held_preserves_fresh_click_and_drag() {
    let _guard = headless_test_guard();
    cancel_held_operation(8, [180, 200, 320, 240]);
}

#[test]
fn x11_csd_resize_cancel_while_held_preserves_fresh_click_and_drag() {
    let _guard = headless_test_guard();
    cancel_held_operation(4, [100, 150, 400, 290]);
}

fn reject_wrong_button(direction: u32) {
    let (compositor, mut client) = fixture();
    let point = press(&compositor, &mut client);
    // The actual implicit grab belongs to X button 1, not the requested 3.
    client.request(&compositor, direction, 3, point.root);
    motion(&compositor, point.screen.0 + 60, point.screen.1 + 30);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [100, 150, 320, 240], RED);
    action(&compositor, "pointer-button-up 272");
    client.wait_release();
    // A stale request cannot revive the sequence after its release, either.
    client.request(&compositor, direction, 1, point.root);
    motion(&compositor, 500, 400);
    client.barrier(&compositor);
    wait_geometry(&compositor, &mut client, [100, 150, 320, 240], RED);
    responsive_click(&compositor, &mut client);
}

#[test]
fn x11_csd_move_rejects_wrong_button_and_released_grab() {
    let _guard = headless_test_guard();
    reject_wrong_button(8);
}

#[test]
fn x11_csd_resize_rejects_wrong_button_and_released_grab() {
    let _guard = headless_test_guard();
    reject_wrong_button(4);
}
