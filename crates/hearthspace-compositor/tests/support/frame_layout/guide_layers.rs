//! Frame tools occlude guides and drop highlights while keeping their own outline.
use super::*;
use hearthspace_common::frame_layout::{frame_group_app_id, frame_widget_app_id};

const NOTICE: [u8; 4] = [220, 80, 35, 255];
const BACKING: [u8; 4] = [40, 180, 80, 255];

fn pixels(compositor: &HeadlessCompositor) -> (Vec<u8>, usize, usize) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut pixels).unwrap();
    (pixels, info.width as usize, info.color_type.samples())
}

fn assert_clear(compositor: &HeadlessCompositor, x: usize, y: usize, color: [u8; 4]) {
    let (pixels, width, stride) = pixels(compositor);
    for y in y..y + 5 {
        for x in x..x + 3 {
            let start = (y * width + x) * stride;
            let actual = &pixels[start..start + 3];
            let expected = [color[2], color[1], color[0]];
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
                "editing overlay painted over the tool at ({x},{y}): {actual:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn frame_guides_and_drop_highlights_stay_under_controls_and_group_material() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    client.add(
        frame_widget_app_id(Some(&output), "notifications"),
        (300, 54),
        (0, 0),
        NOTICE,
    );
    client.add(
        frame_widget_app_id(Some(&output), "tools"),
        (246, 54),
        (0, 0),
        [45, 40, 180, 255],
    );
    let group = layout(&compositor)["outputs"][0]["groups"][0]["rect"].clone();
    client.add(
        frame_group_app_id(Some(&output), "system"),
        (
            group["width"].as_f64().unwrap() as i32,
            group["height"].as_f64().unwrap() as i32,
        ),
        (0, 0),
        BACKING,
    );
    action(&compositor, "pointer-motion-abs 700 400");
    action(&compositor, "layout-tool");
    // Both guide boundaries cross the solid middle of these native surfaces.
    assert_clear(&compositor, 79, 48, NOTICE);
    assert_clear(&compositor, 1191, 48, BACKING);

    command(
        &compositor,
        "select",
        json!({"kind":"widget","id":"notifications"}),
        Value::Null,
    );
    let (pixels, width, stride) = pixels(&compositor);
    assert!(
        (32..65).any(|y| (19..22).any(|x| {
            let color = &pixels[(y * width + x) * stride..][..3];
            color[0] > color[1].saturating_add(40) && color[0] > color[2].saturating_add(70)
        })),
        "the control's own orange selection outline must stay visible"
    );

    action(&compositor, "pointer-motion-abs 60 748");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 200 50");
    // The dragged tool covers the right side; the resting drop target remains
    // untouched where its glass is not covered by that raised native surface.
    assert_clear(&compositor, 79, 48, NOTICE);
    assert_clear(&compositor, 110, 48, NOTICE);
    assert_clear(&compositor, 1191, 48, BACKING);
    keys(&compositor, &[1]);
    action(&compositor, "pointer-button-up 272");
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
