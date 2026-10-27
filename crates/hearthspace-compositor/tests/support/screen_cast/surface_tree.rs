//! Capture follows committed content, including viewport and subsurface clients.
use super::*;
use wayland_client::protocol::wl_output::Transform;

#[test]
fn stretched_root_uses_subsurface_density_and_preserves_rotated_buffer_pixels() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    let number = app.add("capture.tree-density".into(), SIZE, (0, 0), RED);
    let root = app.capture_stretched_root(number);
    let mut client = Client::connect(&socket(&compositor));
    let handle = client.toplevel("capture.tree-density");
    let session = client.session(&handle, false);
    // A stretched 1x1 background alone still needs a logical-sized stream.
    let background = client.capture(&session);
    assert_eq!(background.size, SIZE);
    assert_pixels(&background, |_, _| 0xffff0000);
    drop(background);
    let child = app.capture_child(&root, (0, 0), SIZE, SIZE, Transform::Normal, |x, y| {
        if (8..28).contains(&x) && (12..28).contains(&y) {
            GREEN
        } else {
            BLUE
        }
    });
    let normal = client.capture(&session);
    assert_eq!(normal.size, SIZE);
    assert_pixels(&normal, |x, y| {
        if (8..28).contains(&x) && (12..28).contains(&y) {
            0xff00ff00
        } else {
            0xff0000ff
        }
    });
    drop(normal);
    // After a quarter turn this is 640x600 samples over 320x200 logical
    // pixels (2x horizontally, 3x vertically). A tiny white mark verifies
    // detail and orientation, not just a solid-color resized rectangle.
    app.capture_paint(&child, (600, 640), Transform::_90, |x, y| {
        if (570..573).contains(&x) && (16..18).contains(&y) {
            [255; 4]
        } else if x < 300 {
            RED
        } else if y < 320 {
            GREEN
        } else {
            BLUE
        }
    });
    action(&compositor, "zoom-to 0.5");
    let rotated = client.capture(&session);
    assert_eq!(rotated.size, (640, 600));
    // _90 maps capture (x, y) to buffer (y, 639 - x).
    assert_pixels(&rotated, |x, y| {
        if (622..624).contains(&x) && (570..573).contains(&y) {
            0xffffffff
        } else if y < 300 {
            0xffff0000
        } else if x >= 320 {
            0xff00ff00
        } else {
            0xff0000ff
        }
    });
}

#[test]
fn xdg_capture_intersects_requested_geometry_with_committed_surface_tree() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    let number = app.add("capture.effective-geometry".into(), SIZE, (0, 0), RED);
    app.repaint_rectangle(number, RED, (20, 30, 40, 50), GREEN);
    app.capture_geometry(number, (-10, -10, 340, 220));
    let mut client = Client::connect(&socket(&compositor));
    let handle = client.toplevel("capture.effective-geometry");
    let session = client.session(&handle, false);
    let expected = |x, y| {
        if (20..60).contains(&x) && (30..80).contains(&y) {
            0xff00ff00
        } else {
            0xffff0000
        }
    };
    let whole = client.capture(&session);
    assert_eq!(whole.size, SIZE);
    assert_pixels(&whole, expected);
    drop(whole);
    app.capture_geometry(number, (10, 20, 340, 220));
    let cropped = client.capture(&session);
    assert_eq!(cropped.size, (310, 180));
    assert_pixels(&cropped, |x, y| expected(x + 10, y + 20));
    drop(cropped);
    // Smithay falls back to the committed bbox if there is no intersection.
    app.capture_geometry(number, (500, 500, 20, 20));
    let fallback = client.capture(&session);
    assert_eq!(fallback.size, SIZE);
    assert_pixels(&fallback, expected);
}

#[test]
fn painted_cursor_on_owned_popup_subsurface_is_local_and_never_follows_other_windows() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    let selected = app.add("capture.popup-cursor".into(), SIZE, (0, 0), RED);
    let other = app.add("capture.cursor-other".into(), (200, 200), (0, 0), BLUE);
    app.capture_title(selected, "Cursor owner");
    app.capture_title(other, "Other cursor owner");
    let windows = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .clone();
    let selected_id = windows
        .iter()
        .find(|w| w["title"] == "Cursor owner")
        .unwrap()["id"]
        .clone();
    let other_id = windows
        .iter()
        .find(|w| w["title"] == "Other cursor owner")
        .unwrap()["id"]
        .clone();
    action(&compositor, format!("move-window {selected_id} 100 140"));
    action(&compositor, format!("move-window {other_id} 500 140"));
    let (_popup, surface) = app.capture_popup(selected, (40, 50), (60, 50), BLUE);
    let child = app.capture_child(
        &surface,
        (5, 6),
        (30, 20),
        (30, 20),
        Transform::Normal,
        |_, _| GREEN,
    );
    let windows = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .clone();
    let owner = windows.iter().find(|w| w["id"] == selected_id).unwrap();
    let x = owner["x"].as_i64().unwrap() + 55;
    let y = owner["y"].as_i64().unwrap() + owner["titlebar_height"].as_i64().unwrap() + 66;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    app.capture_cursor(&child); // Assert wl_pointer entered this exact subsurface.
    let mut client = Client::connect(&socket(&compositor));
    let handle = client.toplevel("capture.popup-cursor");
    let plain = client.session(&handle, false);
    let painted = client.session(&handle, true);
    let background = |x, y| {
        if (45..75).contains(&x) && (56..76).contains(&y) {
            0xff00ff00
        } else if (40..100).contains(&x) && (50..100).contains(&y) {
            0xff0000ff
        } else {
            0xffff0000
        }
    };
    assert_pixels(&client.capture(&plain), background);
    assert_pixels(&client.capture(&painted), |x, y| {
        if (53..61).contains(&x) && (63..71).contains(&y) {
            0xffffff00
        } else {
            background(x, y)
        }
    });
    action(&compositor, "pointer-motion-abs 560 230");
    assert_pixels(&client.capture(&painted), background);
}
