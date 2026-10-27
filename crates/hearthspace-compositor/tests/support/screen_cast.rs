//! Standard protocol clients inspect selected-window SHM pixels in the real compositor.
use super::{
    canvas_map_client::MapClient,
    navigation::{action, query},
    *,
};

#[path = "screen_cast/client.rs"]
mod client;
#[path = "screen_cast/lifecycle.rs"]
mod lifecycle;
#[path = "screen_cast/surface_tree.rs"]
mod surface_tree;
#[cfg(feature = "xwayland")]
#[path = "screen_cast/x11.rs"]
mod x11;
use client::{Capture, Client};

const RED: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [255, 0, 0, 255];
const SIZE: (i32, i32) = (320, 200);

fn compositor() -> HeadlessCompositor {
    let mut compositor =
        HeadlessCompositor::spawn_with_command("800x600", None, None, 1, false, |command| {
            command.arg("--screen-cast");
        });
    drop(compositor.wait_for_socket());
    compositor
}

fn socket(compositor: &HeadlessCompositor) -> PathBuf {
    compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()))
}

fn assert_pixels(capture: &Capture, expected: impl Fn(i32, i32) -> u32) {
    for (i, pixel) in capture.pixels().into_iter().enumerate() {
        let (x, y) = (i as i32 % capture.size.0, i as i32 / capture.size.0);
        assert_eq!(
            pixel,
            expected(x, y),
            "pixel ({x}, {y}) in {:?}",
            capture.size
        );
    }
}

#[test]
fn screen_cast_globals_require_enablement_and_do_not_enable_virtual_input() {
    let _guard = headless_test_guard();
    for enabled in [false, true] {
        let mut compositor = if enabled {
            compositor()
        } else {
            let mut c = HeadlessCompositor::spawn_with_size("800x600");
            drop(c.wait_for_socket());
            c
        };
        let client = Client::connect(&socket(&compositor));
        for global in [
            "ext_foreign_toplevel_list_v1",
            "ext_foreign_toplevel_image_capture_source_manager_v1",
            "ext_output_image_capture_source_manager_v1",
            "ext_image_copy_capture_manager_v1",
        ] {
            assert_eq!(
                client.state.globals.iter().any(|g| g == global),
                enabled,
                "{global}"
            );
        }
        assert!(
            !client
                .state
                .globals
                .iter()
                .any(|g| g == "zwlr_virtual_pointer_manager_v1"
                    || g == "zwp_virtual_keyboard_manager_v1")
        );
        assert!(compositor.child.try_wait().unwrap().is_none());
    }
}

#[test]
fn selected_window_pixels_exclude_overlap_shell_and_canvas_transform() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    let selected = app.add("capture.same-app".into(), SIZE, (12, 8), RED);
    app.capture_title(selected, "Selected window");
    app.repaint_rectangle(selected, RED, (8, 12, 20, 16), GREEN);
    let covered = app.add("capture.same-app".into(), SIZE, (12, 8), BLUE);
    app.capture_title(covered, "Occluder from the same app");
    app.add(
        hearthspace_common::SHELL_OUTPUT_WATCHER_APP_ID.into(),
        (40, 40),
        (0, 0),
        GREEN,
    );
    let windows = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .clone();
    for window in &windows {
        action(&compositor, format!("move-window {} 100 150", window["id"]));
    }
    let selected_id = windows
        .iter()
        .find(|w| w["title"] == "Selected window")
        .unwrap()["id"]
        .clone();
    let occluder_id = windows
        .iter()
        .find(|w| w["title"] == "Occluder from the same app")
        .unwrap()["id"]
        .clone();
    action(&compositor, format!("focus-window {occluder_id}"));
    let mut client = Client::connect(&socket(&compositor));
    assert_eq!(
        client.state.toplevels.iter().filter(|t| !t.closed).count(),
        2
    );
    let handle = client
        .state
        .toplevels
        .iter()
        .find(|t| t.title == "Selected window")
        .unwrap()
        .clone();
    let session = client.session(&handle, false);
    let frame = client.capture(&session);
    assert_eq!(frame.size, SIZE);
    let expected = |x, y| {
        if (8..28).contains(&x) && (12..28).contains(&y) {
            0xff00ff00
        } else {
            0xffff0000
        }
    };
    assert_pixels(&frame, expected);
    drop(frame);
    let output = client.output_session();
    assert!(
        client.capture(&output).pixels().contains(&0xff0000ff),
        "occluder must be visible on the actual output"
    );
    // Focus/cursor on an overlapping, unselected surface must not enter capture.
    action(&compositor, "pointer-motion-abs 180 230");
    let cursor_session = client.session(&handle, true);
    assert_pixels(&client.capture(&cursor_session), expected);
    action(&compositor, "zoom-to 0.5");
    action(&compositor, format!("move-window {selected_id} 2400 -1600"));
    let zoomed = client.capture(&session);
    assert_eq!(zoomed.size, SIZE);
    assert_pixels(&zoomed, expected);
    drop(zoomed);
    // Transparent client content is composited over black, never desktop pixels.
    app.repaint_rectangle(selected, [0; 4], (8, 12, 20, 16), RED);
    assert_pixels(&client.capture(&session), |x, y| {
        if (8..28).contains(&x) && (12..28).contains(&y) {
            0xffff0000
        } else {
            0xff000000
        }
    });
}

#[test]
fn window_capture_uses_backing_density_and_only_owned_clipped_popups() {
    let _guard = headless_test_guard();
    let compositor = compositor();
    let mut app = MapClient::connect(&socket(&compositor));
    let selected = app.add("capture.popups".into(), SIZE, (12, 8), RED);
    let other = app.add("capture.other".into(), SIZE, (0, 0), BLUE);
    let _unselected = app.capture_popup(other, (40, 50), (40, 30), BLUE);
    let _owned = app.capture_popup(selected, (40, 50), (40, 30), GREEN);
    let _clipped = app.capture_popup(selected, (310, 190), (40, 30), GREEN);
    let mut client = Client::connect(&socket(&compositor));
    let handle = client.toplevel("capture.popups");
    let session = client.session(&handle, false);
    let expected = |x, y| {
        if ((40..80).contains(&x) && (50..80).contains(&y)) || (x >= 310 && y >= 190) {
            0xff00ff00
        } else {
            0xffff0000
        }
    };
    assert_pixels(&client.capture(&session), expected);
    app.capture_scale(selected, 2);
    action(&compositor, "zoom-to 0.5");
    let scaled = client.capture(&session);
    assert_eq!(scaled.size, (640, 400));
    assert_pixels(&scaled, |x, y| expected(x / 2, y / 2));
}
