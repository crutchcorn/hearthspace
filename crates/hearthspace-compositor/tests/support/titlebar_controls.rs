//! Exercise compositor chrome against real asynchronously configured SHM windows.
use super::interactive::client::InteractiveClient;
use super::navigation::{action, query, wait_windows};
use super::*;
use serde_json::Value;

#[path = "focused_view.rs"]
mod focused_view;

fn fixture(scale: i32) -> (HeadlessCompositor, InteractiveClient, u64) {
    fixture_with_size(scale, 1280, 800)
}

fn fixture_with_size(
    scale: i32,
    width: i32,
    height: i32,
) -> (HeadlessCompositor, InteractiveClient, u64) {
    let physical = format!("{}x{}", width * scale, height * scale);
    let mut compositor = HeadlessCompositor::spawn_with_options(&physical, None, None, scale);
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let client = InteractiveClient::connect_server_chrome(&socket);
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let mut first_id = 0;
    for window in windows {
        let first = window["title"] == "Interactive fixture 0";
        let x = if first { 120 } else { 650 };
        action(&compositor, format!("move-window {} {x} 200", window["id"]));
        assert_eq!(window["titlebar_height"], 44);
        if first {
            first_id = window["id"].as_u64().unwrap();
        }
    }
    (compositor, client, first_id)
}

fn window(compositor: &HeadlessCompositor, id: u64) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap()
        .clone()
}

fn motion(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
}

fn button_point(window: &Value, maximize: bool) -> (f64, f64) {
    let scale = window["titlebar_height"].as_f64().unwrap() / 44.0;
    let inset = if maximize { 58.0 } else { 22.0 };
    (
        window["x"].as_f64().unwrap() + window["width"].as_f64().unwrap() - inset * scale,
        window["y"].as_f64().unwrap() + 22.0 * scale,
    )
}

fn click(compositor: &HeadlessCompositor, point: (f64, f64)) {
    motion(compositor, point);
    action(compositor, "pointer-button-down 272");
    action(compositor, "pointer-button-up 272");
}

fn double_click_title(compositor: &HeadlessCompositor, window: &Value) {
    let scale = window["titlebar_height"].as_f64().unwrap() / 44.0;
    let point = (
        window["x"].as_f64().unwrap() + 40.0 * scale,
        window["y"].as_f64().unwrap() + 22.0 * scale,
    );
    click(compositor, point);
    click(compositor, point);
}

fn assert_presentation(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    id: u64,
    maximized: bool,
) -> Value {
    client.sync();
    let windows = wait_windows(compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == id && window["maximized"] == maximized)
    });
    windows
        .into_iter()
        .find(|window| window["id"] == id)
        .unwrap()
}

#[test]
fn titlebar_controls_activate_on_release_cancel_outside_and_restore_at_canvas_and_output_scales() {
    let _guard = headless_test_guard();
    for output_scale in [1, 2] {
        let (mut compositor, mut client, id) = fixture(output_scale);
        if output_scale == 1 {
            action(&compositor, "zoom-out");
            let deadline = Instant::now() + Duration::from_secs(5);
            while query(&compositor, "status")["viewport"]["scale"] != 0.8 {
                assert!(Instant::now() < deadline, "canvas zoom must settle");
                thread::sleep(Duration::from_millis(20));
            }
        }
        let original = window(&compositor, id);
        let maximize = button_point(&original, true);
        motion(&compositor, maximize);
        action(&compositor, "pointer-button-down 272");
        assert!(
            !client.received_pointer_press(),
            "chrome press leaked to client"
        );
        assert_presentation(&compositor, &mut client, id, false);
        motion(&compositor, (maximize.0 - 100.0, maximize.1 + 80.0));
        action(&compositor, "pointer-button-up 272");
        assert_presentation(&compositor, &mut client, id, false);
        assert!(
            client.released_buttons().is_empty(),
            "cancel leaked unmatched release"
        );

        click(&compositor, maximize);
        let maximized = assert_presentation(&compositor, &mut client, id, true);
        assert_eq!(
            maximized["titlebar_height"], 44,
            "pinned chrome must ignore canvas zoom"
        );
        assert_eq!(maximized["width"], 1280);
        assert_eq!(maximized["focused_view"], true);
        click(&compositor, button_point(&maximized, true));
        let restored = assert_presentation(&compositor, &mut client, id, false);
        assert_eq!(restored["focused_view"], false);
        for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
            assert_eq!(restored[field], original[field], "restore {field}");
        }

        let close = button_point(&restored, false);
        motion(&compositor, close);
        action(&compositor, "pointer-button-down 272");
        assert!(client.closed_windows().is_empty(), "close fired on press");
        motion(&compositor, button_point(&restored, true));
        action(&compositor, "pointer-button-up 272");
        assert!(
            client.closed_windows().is_empty(),
            "release over another control closed"
        );
        assert_presentation(&compositor, &mut client, id, false);
        click(&compositor, close);
        assert_eq!(
            client.closed_windows(),
            &[0],
            "close must target the originally pressed window"
        );
        assert!(!client.received_pointer_press());
        assert!(client.released_buttons().is_empty());
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
}

#[test]
fn titlebar_drag_and_double_click_preserve_restore_geometry_and_ignore_disappearing_targets() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(1);
    let original = window(&compositor, id);
    let title = (
        original["x"].as_f64().unwrap() + 40.0,
        original["y"].as_f64().unwrap() + 22.0,
    );
    motion(&compositor, title);
    action(&compositor, "pointer-button-down 272");
    let moved_title = (title.0 + 30.0, title.1 + 18.0);
    motion(&compositor, moved_title);
    action(&compositor, "pointer-button-up 272");
    let dragged = window(&compositor, id);
    assert_eq!(
        dragged["canvas_x"].as_i64().unwrap(),
        original["canvas_x"].as_i64().unwrap() + 30
    );
    assert_eq!(
        dragged["canvas_y"].as_i64().unwrap(),
        original["canvas_y"].as_i64().unwrap() + 18
    );
    click(&compositor, moved_title);
    assert_presentation(&compositor, &mut client, id, false);
    click(&compositor, moved_title);
    let maximized = assert_presentation(&compositor, &mut client, id, true);
    assert_eq!(maximized["focused_view"], false);
    assert_eq!(maximized["x"], 24);
    assert_eq!(maximized["y"], 96);
    assert_eq!(maximized["width"], 1232);
    assert_eq!(maximized["height"], 608);
    let pinned_title = (
        maximized["x"].as_f64().unwrap() + 40.0,
        maximized["y"].as_f64().unwrap() + 22.0,
    );
    click(&compositor, pinned_title);
    click(&compositor, pinned_title);
    let restored = assert_presentation(&compositor, &mut client, id, false);
    assert_eq!(restored["canvas_x"], dragged["canvas_x"]);
    assert_eq!(restored["canvas_y"], dragged["canvas_y"]);

    motion(&compositor, button_point(&restored, false));
    action(&compositor, "pointer-button-down 272");
    client.unmap(0);
    let remaining = wait_windows(&compositor, |windows| windows.len() == 1);
    motion(&compositor, button_point(&remaining[0], false));
    action(&compositor, "pointer-button-up 272");
    assert!(
        client.closed_windows().is_empty(),
        "vanished control activated another window"
    );
    assert!(
        client.released_buttons().is_empty(),
        "vanished control leaked its release"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn titlebar_pinned_controls_match_paint_before_the_client_handles_its_resize() {
    let _guard = headless_test_guard();
    for output_scale in [1, 2] {
        let (mut compositor, mut client, id) = fixture(output_scale);
        let original = window(&compositor, id);
        assert_eq!(original["content_width"], 240);
        assert_eq!(
            original["width"], 260,
            "floating chrome has a minimum width"
        );
        click(&compositor, button_point(&original, true));

        // Deliberately do not dispatch the client's configure: the current
        // output-pinned frame is still its 240px committed content, although its
        // floating chrome was 260px. Painting must use the pinned hit geometry.
        let pending = window(&compositor, id);
        assert_eq!(pending["maximized"], true);
        assert_eq!(pending["focused_view"], true);
        assert_eq!(pending["width"], 240);
        assert_eq!(pending["content_width"], 240);
        let close = button_point(&pending, false);
        motion(&compositor, close);
        let decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()));
        let mut reader = decoder.read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        // One logical pixel inside the left edge, at its vertical center,
        // avoids the glyph and cursor. Stretching the 260px raster moves this
        // red edge three pixels right, away from its actual clickable region.
        let x = ((close.0 - 15.0) * f64::from(output_scale)) as usize;
        let y = (close.1 * f64::from(output_scale)) as usize;
        let offset = (y * info.width as usize + x) * info.color_type.samples();
        let [red, green, blue]: [u8; 3] = data[offset..offset + 3].try_into().unwrap();
        assert!(
            red > 150 && green < 110 && blue < 90,
            "close hover must cover its hit rectangle before configure at {output_scale}×: rgb=({red},{green},{blue})"
        );
        click(&compositor, button_point(&pending, true));
        let restored = assert_presentation(&compositor, &mut client, id, false);
        assert_eq!(restored["focused_view"], false);
        assert_eq!(restored["content_width"], original["content_width"]);
        assert_eq!(restored["canvas_x"], original["canvas_x"]);
        assert!(client.closed_windows().is_empty());
        assert!(client.released_buttons().is_empty());
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
}

#[test]
fn titlebar_controls_do_not_swallow_client_grab_releases() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(1);
    let original = window(&compositor, id);
    let content = (
        original["x"].as_f64().unwrap() + 40.0,
        original["y"].as_f64().unwrap() + original["titlebar_height"].as_f64().unwrap() + 40.0,
    );
    for (maximize, name) in [(false, "close"), (true, "maximize")] {
        let before = client.released_buttons().len();
        motion(&compositor, content);
        action(&compositor, "pointer-button-down 272");
        client.press_serial();
        // This is an application-owned implicit grab, such as selecting text.
        // Moving across compositor controls must retain its matching release.
        motion(&compositor, button_point(&original, maximize));
        action(&compositor, "pointer-button-up 272");
        assert_eq!(
            &client.released_buttons()[before..],
            &[272],
            "the client must receive its release over the {name} control"
        );
        assert!(client.closed_windows().is_empty());
        assert_presentation(&compositor, &mut client, id, false);
    }
    // A subsequent chrome click must work, proving no implicit grab survived.
    click(&compositor, button_point(&original, true));
    assert_presentation(&compositor, &mut client, id, true);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn maximized_titlebar_keeps_its_release_when_the_pointer_moves_into_content() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(1);
    let original = window(&compositor, id);
    click(&compositor, button_point(&original, true));
    let maximized = assert_presentation(&compositor, &mut client, id, true);
    let title = (
        maximized["x"].as_f64().unwrap() + 40.0,
        maximized["y"].as_f64().unwrap() + 22.0,
    );
    let content = (title.0, title.1 + 80.0);
    motion(&compositor, title);
    action(&compositor, "pointer-button-down 272");
    motion(&compositor, content);
    action(&compositor, "pointer-button-up 272");
    assert!(!client.received_pointer_press());
    assert!(
        client.released_buttons().is_empty(),
        "the client must not receive the maximized titlebar's unmatched release"
    );
    assert_presentation(&compositor, &mut client, id, true);

    // Ordinary app input must recover immediately after the owned sequence.
    motion(&compositor, content);
    action(&compositor, "pointer-button-down 272");
    client.press_serial();
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.released_buttons(), &[272]);
    click(&compositor, button_point(&maximized, true));
    let restored = assert_presentation(&compositor, &mut client, id, false);
    assert_eq!(restored["focused_view"], false);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn existing_client_grabs_keep_additional_button_presses_over_all_chrome() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(1);
    let original = window(&compositor, id);
    let x = original["x"].as_f64().unwrap();
    let y = original["y"].as_f64().unwrap();
    let content = (x + 40.0, y + 84.0);
    for (name, target) in [
        ("close", button_point(&original, false)),
        ("maximize", button_point(&original, true)),
        ("title", (x + 40.0, y + 22.0)),
        (
            "resize border",
            (x + original["width"].as_f64().unwrap() + 2.0, y + 84.0),
        ),
    ] {
        let before = client.released_buttons().len();
        motion(&compositor, content);
        action(&compositor, "pointer-button-down 273");
        let first_press = client.press_serial();
        motion(&compositor, target);
        action(&compositor, "pointer-button-down 272");
        assert_ne!(
            client.press_serial(),
            first_press,
            "the existing client grab must receive the left press over {name}"
        );
        motion(&compositor, (target.0 + 12.0, target.1 + 12.0));
        action(&compositor, "pointer-button-up 272");
        action(&compositor, "pointer-button-up 273");
        assert_eq!(&client.released_buttons()[before..], &[272, 273]);
        let unchanged = assert_presentation(&compositor, &mut client, id, false);
        for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
            assert_eq!(unchanged[field], original[field], "{name} changed {field}");
        }
        assert!(client.closed_windows().is_empty());
    }
    click(&compositor, button_point(&original, true));
    assert_presentation(&compositor, &mut client, id, true);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
