use super::*;
use crate::session_lock::lock_client::LockClient;

#[test]
fn canvas_pan_tool_change_cancels_motion_but_drains_old_buttons() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, "canvas-tool pan");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    action(&compositor, "canvas-tool select");
    assert_eq!(tool(&compositor), "select");
    motion(&compositor, (600., 600.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [-50, -50]);
    assert_buttons(&client.take_pointer_events(), &[]);
    assert_eq!(
        query(&compositor, "desktop-status")["items"],
        serde_json::json!([])
    );
    action(&compositor, "canvas-tool pan");
    start(&compositor, (600., 600.));
    motion(&compositor, (620., 620.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [-70, -70]);
}

#[test]
fn canvas_pan_reset_and_lock_discard_late_releases_without_moving_view() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, "canvas-tool pan");
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    action(&compositor, "pointer-button-down 273");
    action(&compositor, "session-resume");
    let stopped = offset(&compositor);
    finish(&compositor);
    action(&compositor, "pointer-button-up 273");
    motion(&compositor, (600., 600.));
    assert_eq!(offset(&compositor), stopped);
    assert_buttons(&client.take_pointer_events(), &[]);
    action(&compositor, "canvas-tool pan");
    start(&compositor, (600., 600.));
    motion(&compositor, (620., 620.));
    let stopped = offset(&compositor);
    let mut locker = LockClient::connect(&socket(&compositor));
    locker.wait_locked();
    assert_eq!(tool(&compositor), Value::Null);
    assert!(
        compositor
            .send_text_command("canvas-tool pan")
            .starts_with("err")
    );
    locker.unlock();
    finish(&compositor);
    motion(&compositor, (650., 650.));
    assert_eq!(offset(&compositor), stopped);
    assert_buttons(&client.take_pointer_events(), &[]);
    action(&compositor, "canvas-tool pan");
    start(&compositor, (650., 650.));
    motion(&compositor, (660., 660.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [stopped[0] - 10, stopped[1] - 10]);
}

#[test]
fn canvas_pan_refuses_existing_client_grab_and_preserves_its_release() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    action(&compositor, "canvas-tool pan");
    start(&compositor, (140., 140.));
    let serial = client.press_serial();
    client.request_move(0, serial);
    motion(&compositor, (500., 500.));
    let before = viewport(&compositor);
    action(&compositor, "pointer-button-down 273");
    motion(&compositor, (550., 550.));
    finish(&compositor);
    action(&compositor, "pointer-button-up 273");
    assert_eq!(viewport(&compositor), before);
    // The window operation still moved its original owner, not the viewport.
    let windows = query(&compositor, "windows");
    let moved = windows["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Interactive fixture 0")
        .unwrap();
    assert_eq!(moved["canvas_x"], 510);
    assert_eq!(moved["canvas_y"], 510);
}

#[test]
fn canvas_pan_outside_press_dismisses_shell_palette_and_recovers_app_focus() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = crate::shell_panels::connect(&compositor);
    let app = client.add("pan-app".into(), (200, 120), (0, 0), [40, 100, 180, 255]);
    action(&compositor, "canvas-tool pan");
    let panel = client.add(
        hearthspace_common::TRAY_MENU_APP_ID.into(),
        (250, 150),
        (0, 0),
        [40, 180, 100, 255],
    );
    assert_eq!(client.focus(), Some(panel));
    let epoch = query(&compositor, "layout-status")["popup_epoch"].clone();
    start(&compositor, (20., 500.));
    motion(&compositor, (70., 550.));
    assert!(client.closed().contains(&panel));
    assert_eq!(
        client.focus(),
        Some(app),
        "closing palette stranded keyboard focus"
    );
    assert_ne!(query(&compositor, "layout-status")["popup_epoch"], epoch);
    finish(&compositor);
    assert_eq!(offset(&compositor), [-50, -50]);
    // MapClient records both phases. Check each boundary so duplicate presses
    // cannot masquerade as the expected complete key pair.
    action(&compositor, "key-down 30");
    assert_eq!(client.keys(), &[30]);
    assert_eq!(client.focus(), Some(app));
    action(&compositor, "key-up 30");
    assert_eq!(client.keys(), &[30, 30]);
    assert_eq!(client.focus(), Some(app));
}
