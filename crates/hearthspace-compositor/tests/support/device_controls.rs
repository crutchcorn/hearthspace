//! Protocol-level control/OSD coverage without reading or changing host devices.
use super::navigation::{action, keys, query};
use super::shell_panels::{click, connect};
use super::*;
use hearthspace_common::{CONTROL_OSD_APP_ID, QUICK_PANEL_APP_ID, SHELL_BAR_APP_ID};
use serde_json::{Value, json};
use std::os::unix::net::UnixDatagram;

const GREEN: [u8; 4] = [0x66, 0xcc, 0, 0xff];

fn green_pixel(compositor: &HeadlessCompositor, x: usize, y: usize) {
    let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    let index = (y * info.width as usize + x) * info.color_type.samples();
    assert_eq!(
        bytes[index..index + 3],
        [0, 0xcc, 0x66],
        "OSD pixel {x},{y}"
    );
}

#[test]
fn osd_is_fixed_passive_above_fullscreen_and_never_becomes_a_window_or_popup() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.OsdTest".into(),
        (1280, 800),
        (0, 0),
        [0xff, 0, 0, 0xff],
    );
    let id = query(&compositor, "windows")["windows"][0]["id"]
        .as_u64()
        .unwrap();
    let osd = client.add(CONTROL_OSD_APP_ID.into(), (300, 90), (8, 6), GREEN);
    assert_eq!(
        client.focus(),
        Some(normal),
        "mapping OSD must not steal focus"
    );
    let bounds = json!({"output":null,"x":490,"y":662,"width":300,"height":90});
    assert_eq!(
        query(&compositor, "status")["control_osds"],
        json!([bounds])
    );
    assert_eq!(query(&compositor, "status")["shell_panels"], json!([]));
    assert_eq!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    green_pixel(&compositor, 500, 680);
    action(&compositor, format!("fullscreen-window {id} true"));
    client.sync();
    green_pixel(&compositor, 500, 680);
    click(&compositor, 510.0, 690.0);
    assert_eq!(client.focus(), Some(normal));
    assert_eq!(
        client.presses().last().map(|press| press.0),
        Some(normal),
        "OSD input must pass through"
    );
    keys(&compositor, &[30]);
    assert_eq!(client.keys(), [30, 30]);
    action(&compositor, format!("fullscreen-window {id} false"));
    action(&compositor, "pan-to 9000 -7000");
    assert_eq!(
        query(&compositor, "status")["control_osds"],
        json!([bounds])
    );
    let panel = client.add(
        QUICK_PANEL_APP_ID.into(),
        (340, 250),
        (0, 0),
        [0xff, 0, 0, 0xff],
    );
    assert_eq!(client.focus(), Some(panel));
    client.repaint_with_size(osd, (300, 90));
    assert_eq!(
        client.focus(),
        Some(panel),
        "OSD updates must preserve the open popup"
    );
    assert!(!client.closed().contains(&panel));
    client.destroy(osd);
    assert_eq!(
        client.focus(),
        Some(panel),
        "OSD timeout must preserve the popup"
    );
    assert_eq!(query(&compositor, "status")["control_osds"], json!([]));
    client.destroy(panel);
    action(&compositor, "overview-on");
    let osd = client.add(CONTROL_OSD_APP_ID.into(), (300, 90), (0, 0), GREEN);
    green_pixel(&compositor, 500, 680);
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert!(client.closed().contains(&osd));
    assert_eq!(query(&compositor, "status")["control_osds"], json!([]));
    let late = client.add(CONTROL_OSD_APP_ID.into(), (300, 90), (0, 0), GREEN);
    assert!(
        client.closed().contains(&late),
        "late OSD mapping while locked must be dismissed"
    );
    client.destroy(late);
    client.destroy(osd);
    locker.unlock();
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn osd_fits_scaled_small_outputs_without_taking_keyboard_focus() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("640x480", None, None, 2);
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    client.add(CONTROL_OSD_APP_ID.into(), (500, 440), (0, 0), GREEN);
    assert_eq!(client.focus(), None);
    assert_eq!(
        query(&compositor, "status")["control_osds"],
        json!([
            {"output":null,"x":0,"y":6,"width":320,"height":186}
        ])
    );
    green_pixel(&compositor, 200, 200);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

fn receive(events: &UnixDatagram, control: &str) -> u64 {
    let mut bytes = [0; 4096];
    let count = events.recv(&mut bytes).unwrap();
    let event: Value = serde_json::from_slice(&bytes[..count]).unwrap();
    assert_eq!(event["version"], 1);
    assert_eq!(event["action"], "device-control");
    assert_eq!(event["control"], control);
    assert_eq!(event["output"], Value::Null);
    event["token"].as_u64().unwrap()
}

#[test]
fn media_keys_send_one_use_events_repeat_levels_and_revoke_requests_at_lock() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let events = UnixDatagram::bind(hearthspace_common::shell_event_socket_path(
        &compositor.command_socket_path,
    ))
    .unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.MediaKeyTest".into(),
        (240, 160),
        (0, 0),
        [0xff, 0, 0, 0xff],
    );
    client.add(SHELL_BAR_APP_ID.into(), (1280, 54), (0, 0), GREEN);
    for (code, control) in [
        (115, "volume-up"),
        (114, "volume-down"),
        (113, "volume-mute"),
        (225, "brightness-up"),
        (224, "brightness-down"),
    ] {
        keys(&compositor, &[code]);
        let token = receive(&events, control);
        assert_ne!(token & (1 << 63), 0);
        action(&compositor, format!("shell-event-accept {token}"));
        assert!(
            compositor
                .send_text_command(&format!("shell-event-accept {token}"))
                .starts_with("err ")
        );
        assert_eq!(client.focus(), Some(normal));
    }
    assert!(
        client.keys().is_empty(),
        "presses and releases must both stay out of clients"
    );
    action(&compositor, "key-down 115");
    let initial = receive(&events, "volume-up");
    let repeat = receive(&events, "volume-up");
    assert_ne!(repeat, initial);
    action(&compositor, "key-up 115");
    action(&compositor, format!("shell-event-accept {repeat}"));
    // No key is held: an unacknowledged datagram must expire while queued.
    keys(&compositor, &[113]);
    let expired = receive(&events, "volume-mute");
    thread::sleep(Duration::from_millis(550));
    assert!(
        compositor
            .send_text_command(&format!("shell-event-accept {expired}"))
            .starts_with("err ")
    );
    keys(&compositor, &[113]);
    let cancelled = receive(&events, "volume-mute");
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert!(
        compositor
            .send_text_command(&format!("shell-event-accept {cancelled}"))
            .starts_with("err ")
    );
    locker.unlock();
    assert!(
        compositor
            .send_text_command(&format!("shell-event-accept {cancelled}"))
            .starts_with("err ")
    );
    events
        .set_read_timeout(Some(Duration::from_millis(120)))
        .unwrap();
    assert!(
        events.recv(&mut [0; 4096]).is_err(),
        "release and lock must stop repeated events"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
