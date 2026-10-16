//! Selected outlines participate in window stacking, including opaque overlap.
use super::super::{interactive::client::InteractiveClient, navigation::wait_windows};
use super::*;

struct Screenshot {
    pixels: Vec<u8>,
    width: usize,
    channels: usize,
}

impl Screenshot {
    fn capture(compositor: &HeadlessCompositor, name: &str) -> Self {
        let png = compositor.take_screenshot();
        std::fs::write(compositor.runtime_dir.path().join(name), &png).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        Self {
            pixels,
            width: info.width as usize,
            channels: info.color_type.samples(),
        }
    }

    fn assert_strip(&self, x: usize, y: usize, expected: [u8; 3], label: &str) {
        // Sample several interior pixels along the edge, away from corners,
        // titlebar text, the separate Gather button, and the software cursor.
        for row in y..y + 8 {
            let offset = (row * self.width + x) * self.channels;
            let actual = &self.pixels[offset..offset + 3];
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(&actual, expected)| actual.abs_diff(expected) <= 2),
                "{label}: pixel ({x}, {row}) is {actual:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn overlapping_selected_outlines_follow_window_stacking_and_opaque_occlusion() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect(&socket);
    let mapped = wait_windows(&compositor, |windows| windows.len() == 2);
    let id = |number| {
        mapped
            .iter()
            .find(|window| window["title"] == format!("Interactive fixture {number}"))
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    };
    let (first, second) = (id(0), id(1));
    for (id, x, y) in [(first, 100, 200), (second, 200, 260)] {
        action(&compositor, format!("move-window {id} {x} {y}"));
    }
    action(&compositor, format!("focus-window {first}"));
    action(&compositor, "group-selection-toggle");
    for code in [57, 15, 57, 28] {
        keys(&compositor, &[code]); // Space, Tab, Space, Enter selects both.
    }
    assert_eq!(group(&compositor)["selected"], json!([first, second]));
    action(&compositor, format!("focus-window {second}"));
    action(&compositor, "pan-to 0 0");
    action(&compositor, "pointer-motion-abs 10 700");
    client.sync();

    let a = window(&compositor, first);
    let b = window(&compositor, second);
    let number = |item: &Value, key: &str| item[key].as_u64().unwrap() as usize;
    let y = number(&b, "y") + number(&b, "titlebar_height") + 30;
    let a_left = number(&a, "x") + 1;
    let a_right = number(&a, "x") + number(&a, "width") - 2;
    let b_left = number(&b, "x") + 1;
    let b_right = number(&b, "x") + number(&b, "width") - 2;
    assert!(y > number(&a, "y") + number(&a, "titlebar_height"));
    assert!(y + 8 < number(&a, "y") + number(&a, "height") - 3);
    assert!(a_left < b_left && b_left + 3 < a_right && a_right + 3 < b_right);
    let opaque_client = [0, 204, 102]; // InteractiveClient's opaque ARGB buffer.
    let selected_outline = [117, 171, 255];
    let second_in_front = Screenshot::capture(&compositor, "group-second-in-front.png");
    second_in_front.assert_strip(
        a_right,
        y,
        opaque_client,
        "foreground client must cover the rear selected window's right outline",
    );
    for x in [a_left, b_left, b_right] {
        second_in_front.assert_strip(x, y, selected_outline, "exposed selected outline");
    }

    // Raising the rear member must reverse occlusion at exactly the same pixels.
    action(&compositor, format!("focus-window {first}"));
    client.sync();
    let first_in_front = Screenshot::capture(&compositor, "group-first-in-front.png");
    first_in_front.assert_strip(
        b_left,
        y,
        opaque_client,
        "raising a member must hide the other member's previously visible left outline",
    );
    for x in [a_left, a_right, b_right] {
        first_in_front.assert_strip(x, y, selected_outline, "outline after stacking changes");
    }

    // An unselected normal window also occludes selected windows below it.
    action(&compositor, "group-selection-toggle");
    for code in [15, 57, 28] {
        keys(&compositor, &[code]); // Focus second, remove it from selection, finish.
    }
    assert_eq!(group(&compositor)["selected"], json!([first]));
    client.sync();
    let unselected_in_front = Screenshot::capture(&compositor, "group-unselected-in-front.png");
    unselected_in_front.assert_strip(a_left, y, selected_outline, "exposed rear selection");
    for x in [a_right, b_left, b_right] {
        unselected_in_front.assert_strip(
            x,
            y,
            opaque_client,
            "unselected foreground client must hide rear outlines and have no outline itself",
        );
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
