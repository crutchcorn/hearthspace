use super::*;
use fixture::{Options, with_options};

fn assert_not_overlay(c: &HeadlessCompositor) {
    let status = query(c, "status");
    let display = status["xwayland_display"].as_str().unwrap();
    let _sentinel = WindowClient::new(c, display, "processed after candidate", 0x404020);
    assert!(
        query(c, "status")["screen_overlays"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn managed_utility_keeps_ordinary_placement() {
    let _guard = headless_test_guard();
    let (c, display) = super::super::fixture();
    let (_connection, _) = with_options(
        &c,
        &display,
        Options {
            unmanaged: false,
            ..Default::default()
        },
    );
    assert_not_overlay(&c);
}

#[test]
fn transient_utility_keeps_its_parent_role() {
    let _guard = headless_test_guard();
    let (c, display) = super::super::fixture();
    let parent = WindowClient::new(&c, &display, "parent", 0x404020);
    let (_connection, _) = with_options(
        &c,
        &display,
        Options {
            parent: Some(parent.window),
            ..Default::default()
        },
    );
    assert_not_overlay(&c);
}

#[test]
fn preferred_normal_type_is_not_overridden_by_a_later_utility_hint() {
    let _guard = headless_test_guard();
    let (c, display) = super::super::fixture();
    let (_connection, _) = with_options(
        &c,
        &display,
        Options {
            prefer_normal: true,
            ..Default::default()
        },
    );
    assert_not_overlay(&c);
}

#[test]
fn destroying_overlay_during_press_does_not_release_into_underlying_window() {
    let _guard = headless_test_guard();
    let (c, display) = super::super::fixture();
    let base = WindowClient::new(&c, &display, "under toast", 0x404020);
    action(&c, format!("move-window {} 1056 696", base.id));
    let (connection, window) = overlay(&c, &display, true, true);
    let rect = overlays(&c, 1).remove(0);
    assert_eq!(rect["x"], 1056);
    assert_eq!(rect["y"], 696);
    base.drain(&mut Vec::new());
    action(&c, "pointer-motion-abs 1076 716");
    action(&c, "pointer-button-down 272");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(Event::ButtonPress(e)) = connection.poll_for_event().unwrap() {
            assert_eq!((e.event, e.event_x, e.event_y), (window, 20, 20));
            break;
        }
        assert!(Instant::now() < deadline, "missing initiating toast press");
        thread::sleep(Duration::from_millis(5));
    }
    connection.destroy_window(window).unwrap();
    connection.flush().unwrap();
    overlays(&c, 0);
    action(&c, "pointer-button-up 272");
    action(&c, "pointer-button-down 272");
    action(&c, "pointer-button-up 272");
    // A new click is a delivery barrier and must be the only balanced pair.
    let events = super::super::wait_click(&base);
    super::super::balanced(&base, &events, (20, 20));
}

#[test]
fn remapped_overlay_uses_new_requested_corner_and_live_role() {
    use smithay::reexports::x11rb::protocol::xproto::ConfigureWindowAux;
    let _guard = headless_test_guard();
    let (c, display) = super::super::fixture();
    let (connection, window) = overlay(&c, &display, true, true);
    assert_eq!(overlays(&c, 1)[0]["x"], 1056);
    connection.unmap_window(window).unwrap();
    connection.flush().unwrap();
    overlays(&c, 0);
    connection
        .configure_window(window, &ConfigureWindowAux::new().x(100).y(100))
        .unwrap();
    connection.map_window(window).unwrap();
    connection.flush().unwrap();
    let rect = overlays(&c, 1).remove(0);
    assert_eq!(rect["x"], 24);
    assert_eq!(rect["y"], 24);
}
