use super::*;
use crate::remote_input::client::RemoteClient;

#[test]
fn canvas_pan_origin_release_stops_motion_before_other_sources_drain() {
    let _guard = headless_test_guard();
    let compositor = fixture(true);
    let mut client = clients(&compositor);
    let mut remote = RemoteClient::connect(&socket(&compositor));
    action(&compositor, "canvas-tool pan");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    assert_eq!(offset(&compositor), [-50, -50]);
    remote.motion(700, 700, 1280, 800);
    assert_eq!(offset(&compositor), [-50, -50], "foreign pointer drove pan");
    remote.button(true);
    finish(&compositor); // Remote still holds the same aggregated button.
    motion(&compositor, (600., 600.));
    assert_eq!(
        offset(&compositor),
        [-50, -50],
        "origin release did not stop pan"
    );
    remote.button(false);
    assert_buttons(&client.take_pointer_events(), &[]);
    start(&compositor, (600., 600.));
    motion(&compositor, (620., 620.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [-70, -70]);
}

#[test]
fn canvas_pan_remote_owner_ignores_local_motion_and_cancels_on_removal() {
    let _guard = headless_test_guard();
    let compositor = fixture(true);
    let mut remote = RemoteClient::connect(&socket(&compositor));
    action(&compositor, "canvas-tool pan");
    remote.motion(500, 500, 1280, 800);
    remote.button(true);
    motion(&compositor, (600., 600.));
    assert_eq!(offset(&compositor), [0, 0]);
    remote.motion(550, 550, 1280, 800);
    assert_eq!(offset(&compositor), [-50, -50]);
    remote.destroy_pointer();
    motion(&compositor, (600., 600.));
    assert_eq!(offset(&compositor), [-50, -50]);
    start(&compositor, (600., 600.));
    motion(&compositor, (610., 610.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [-60, -60]);
}

#[test]
fn canvas_pan_drains_secondary_buttons_and_swallows_scroll_after_escape() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, "canvas-tool pan");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    action(&compositor, "pointer-button-down 273");
    keys(&compositor, &[1]);
    let stopped = viewport(&compositor);
    finish(&compositor);
    action(&compositor, "key-down 125");
    action(&compositor, "axis 0 -120");
    action(&compositor, "key-up 125");
    motion(&compositor, (600., 600.));
    assert_eq!(viewport(&compositor), stopped);
    assert_eq!(tool(&compositor), "pan");
    assert_silent(&mut client, "cancelled pan leaked before secondary release");
    action(&compositor, "pointer-button-up 273");
    assert_buttons(&client.take_pointer_events(), &[]);
    action(&compositor, "canvas-tool select");
    start(&compositor, (200., 200.));
    finish(&compositor);
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Pressed), (272, ButtonState::Released)],
    );
}

#[test]
fn canvas_pan_preserves_direct_touch_and_never_claims_touch_pointer_fallback() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    client.enable_touch();
    action(&compositor, "canvas-tool pan");
    action(&compositor, "touch-down fingers 0 140 140");
    action(&compositor, "touch-frame fingers");
    action(&compositor, "touch-motion fingers 0 170 160");
    action(&compositor, "touch-frame fingers");
    action(&compositor, "touch-up fingers 0");
    action(&compositor, "touch-frame fingers");
    let events = client.take_touch_events();
    use wayland_client::protocol::wl_touch::Event;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Down { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Motion { .. }))
    );
    assert!(events.iter().any(|event| matches!(event, Event::Up { .. })));
    assert!(!events.iter().any(|event| matches!(event, Event::Cancel)));
    assert_buttons(&client.take_pointer_events(), &[]);
    assert_eq!(offset(&compositor), [0, 0]);
    action(&compositor, "touch-down fingers 0 500 500");
    action(&compositor, "touch-frame fingers");
    action(&compositor, "touch-motion fingers 0 600 550");
    action(&compositor, "touch-frame fingers");
    action(&compositor, "touch-up fingers 0");
    action(&compositor, "touch-frame fingers");
    assert_eq!(
        offset(&compositor),
        [0, 0],
        "touch fallback became a pan drag"
    );
}
