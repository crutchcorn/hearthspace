//! Shell map geometry, fixed-size updates, input and popup-like dismissal.
use super::navigation::{action, query};
use super::*;
use hearthspace_common::{
    CANVAS_MAP_APP_ID, canvas_map_app_id_for_output, shell_bar_app_id_for_output,
};
use serde_json::{Value, json};

use super::canvas_map_client::MapClient;

#[path = "canvas_map/glass.rs"]
mod glass;
#[path = "canvas_map/zoom.rs"]
mod zoom;

const GREEN: [u8; 4] = [0x66, 0xcc, 0, 0xff];

fn connect(compositor: &HeadlessCompositor) -> MapClient {
    MapClient::connect(
        &compositor
            .runtime_dir
            .path()
            .join(format!("hearthspace-test-{}", std::process::id())),
    )
}

fn map_bounds(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "status")["canvas_maps"][0].clone()
}

fn click(compositor: &HeadlessCompositor, x: f64, y: f64) {
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 272");
    action(compositor, "pointer-button-up 272");
}

fn wait_animation(compositor: &HeadlessCompositor) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while query(compositor, "status")["viewport"]["animating"] == true {
        assert!(Instant::now() < deadline, "canvas animation timed out");
        thread::sleep(Duration::from_millis(20));
    }
}

fn assert_pixel(compositor: &HeadlessCompositor, x: usize, y: usize, green: bool) {
    let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    let stride = info.color_type.samples();
    let index = (y * info.width as usize + x) * stride;
    assert_eq!(
        bytes[index..index + 3] == [0, 0xcc, 0x66],
        green,
        "pixel {x},{y}"
    );
}

#[test]
fn canvas_map_stays_right_anchored_hit_testable_and_focused_while_zooming() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.MapTest".into(),
        (240, 160),
        (0, 0),
        [0xff, 0, 0, 0xff],
    );
    let bar = client.add(
        shell_bar_app_id_for_output(&output),
        (1280, 54),
        (0, 0),
        [0, 0, 0xff, 0xff],
    );
    let map = client.add(CANVAS_MAP_APP_ID.into(), (340, 336), (8, 6), GREEN);
    assert_eq!(client.focus(), Some(map));
    assert_eq!(
        map_bounds(&compositor),
        json!({"output": null, "x": 928, "y": 66, "width": 340, "height": 336})
    );
    assert_eq!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "map is shell chrome, not a normal window"
    );
    assert_pixel(&compositor, 940, 80, true);
    assert_pixel(&compositor, 915, 80, false);
    click(&compositor, 950.0, 100.0);
    assert_eq!(client.presses().last(), Some(&(map, (30.0, 40.0))));

    client.resize(map, (420, 260));
    let resized = json!({"output": null, "x": 848, "y": 66, "width": 420, "height": 260});
    assert_eq!(map_bounds(&compositor), resized);
    action(&compositor, "pan-right");
    action(&compositor, "zoom-out");
    wait_animation(&compositor);
    assert_eq!(map_bounds(&compositor), resized);
    assert_eq!(client.focus(), Some(map));
    assert!(client.closed().is_empty());
    assert_pixel(&compositor, 860, 80, true);
    click(&compositor, 860.0, 100.0);
    assert_eq!(client.presses().last(), Some(&(map, (20.0, 40.0))));

    // The base-ID map and output-qualified bar form the same focus group.
    click(&compositor, 10.0, 20.0);
    assert_eq!(client.focus(), Some(bar));
    assert!(
        client.closed().is_empty(),
        "toggle press must not dismiss before its release"
    );
    click(&compositor, 860.0, 100.0);
    let viewport = query(&compositor, "status")["viewport"].clone();
    action(&compositor, "key-down 1");
    assert!(client.closed().contains(&map));
    client.destroy(map);
    assert_eq!(client.focus(), Some(normal));
    assert_eq!(
        query(&compositor, "status")["viewport"],
        viewport,
        "closing view controls must not pan back to an offscreen app"
    );
    action(&compositor, "key-up 1");
    assert!(
        client.keys().is_empty(),
        "Escape release must not leak to the restored app"
    );

    let map = client.add(
        canvas_map_app_id_for_output(&output),
        (340, 336),
        (0, 0),
        GREEN,
    );
    click(&compositor, 800.0, 700.0);
    assert!(client.closed().contains(&map));
    assert_eq!(client.focus(), None);
    client.destroy(map);
    assert_eq!(
        client.focus(),
        None,
        "outside background click must retain cleared focus"
    );

    let map = client.add(
        canvas_map_app_id_for_output(&output),
        (340, 336),
        (0, 0),
        GREEN,
    );
    let id = query(&compositor, "windows")["windows"][0]["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("focus-window {id}"));
    assert!(
        client.closed().contains(&map),
        "keyboard/agent navigation dismisses map"
    );
    assert_eq!(client.focus(), Some(normal));
    client.destroy(map);
    assert_eq!(client.focus(), Some(normal));

    wait_animation(&compositor);
    let map = client.add(
        canvas_map_app_id_for_output(&output),
        (340, 336),
        (0, 0),
        GREEN,
    );
    let window = query(&compositor, "windows")["windows"][0].clone();
    click(
        &compositor,
        window["x"].as_f64().unwrap() + 30.0,
        window["y"].as_f64().unwrap() + 30.0,
    );
    assert!(
        client.closed().contains(&map),
        "outside app press dismisses map"
    );
    assert_eq!(client.focus(), Some(normal));
    client.destroy(map);

    let map = client.add(
        canvas_map_app_id_for_output(&output),
        (340, 336),
        (0, 0),
        GREEN,
    );
    action(&compositor, "key-down 56");
    super::navigation::keys(&compositor, &[15]);
    assert!(client.closed().contains(&map));
    assert_eq!(client.focus(), Some(normal));
    client.destroy(map);
    super::navigation::keys(&compositor, &[1]);
    action(&compositor, "key-up 56");

    let map = client.add(
        canvas_map_app_id_for_output(&output),
        (340, 336),
        (0, 0),
        GREEN,
    );
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert!(
        client.closed().contains(&map),
        "lock revokes the map's focus group"
    );
    assert_eq!(query(&compositor, "status")["canvas_maps"], json!([]));
    client.destroy(map);
    locker.unlock();
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn canvas_map_clamps_to_scaled_small_output_and_resizes_in_logical_pixels() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("640x480", None, None, 2);
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let map = client.add(
        canvas_map_app_id_for_output(&output),
        (340, 336),
        (0, 0),
        GREEN,
    );
    assert_eq!(
        map_bounds(&compositor),
        json!({"output": output, "x": 16, "y": 54, "width": 292, "height": 186})
    );
    click(&compositor, 100.0, 100.0);
    assert_eq!(client.presses().last(), Some(&(map, (84.0, 46.0))));
    client.resize(map, (160, 100));
    assert_eq!(
        map_bounds(&compositor),
        json!({"output": output, "x": 148, "y": 66, "width": 160, "height": 100})
    );
    click(&compositor, 180.0, 90.0);
    assert_eq!(client.presses().last(), Some(&(map, (32.0, 24.0))));
    assert!(client.closed().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn absolute_map_pan_is_immediate_preserves_window_geometry_and_respects_modal_input() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    client.add(
        "dev.hearthspace.MapPanTest".into(),
        (240, 160),
        (0, 0),
        [0xff, 0, 0, 0xff],
    );
    let map = client.add(CANVAS_MAP_APP_ID.into(), (340, 336), (0, 0), GREEN);
    action(&compositor, "zoom-out");
    wait_animation(&compositor);
    let window = query(&compositor, "windows")["windows"][0].clone();
    let bounds = map_bounds(&compositor);
    // The actual widget holds its normal implicit pointer grab throughout drag.
    action(&compositor, "pointer-motion-abs 1000 100");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pan-right");
    for (x, y) in [(1234, -567), (1320, -690), (1_000_000, -1_000_000)] {
        action(&compositor, format!("pan-to {x} {y}"));
        let status = query(&compositor, "status");
        assert_eq!(status["viewport"]["x"], x);
        assert_eq!(status["viewport"]["y"], y);
        assert_eq!(status["viewport"]["scale"], 0.8);
        assert_eq!(status["viewport"]["animating"], false);
        assert_eq!(map_bounds(&compositor), bounds);
        let after = &status["windows"][0];
        for key in [
            "canvas_x",
            "canvas_y",
            "content_width",
            "content_height",
            "maximized",
            "fullscreen",
        ] {
            assert_eq!(after[key], window[key], "pan changed window {key}");
        }
        assert_eq!(client.focus(), Some(map));
        assert!(client.closed().is_empty());
    }
    action(&compositor, "pointer-button-up 272");
    let viewport = query(&compositor, "status")["viewport"].clone();
    for invalid in ["pan-to 1000001 0", "pan-to 0 -1000001"] {
        assert!(
            compositor
                .send_text_command(invalid)
                .starts_with("err canvas coordinates")
        );
        assert_eq!(query(&compositor, "status")["viewport"], viewport);
    }
    client.destroy(map);
    action(&compositor, "key-down 56");
    action(&compositor, "key-down 15");
    assert!(
        compositor
            .send_text_command("pan-to 0 0")
            .starts_with("err dismiss the window overlay")
    );
    action(&compositor, "key-up 15");
    action(&compositor, "key-up 56");
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = super::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert!(
        compositor
            .send_text_command("pan-to 0 0")
            .starts_with("err session is locked")
    );
    locker.unlock();
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
