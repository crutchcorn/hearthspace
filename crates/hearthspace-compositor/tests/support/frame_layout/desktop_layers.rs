//! An app occludes desktop editing marks; only the held icon is raised above it.
use super::*;

const APP: [u8; 4] = [190, 70, 20, 255];
const ITEM: [u8; 4] = [20, 190, 70, 255];

fn assert_color(
    compositor: &HeadlessCompositor,
    rect: (usize, usize, usize, usize),
    color: [u8; 4],
    context: &str,
) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut pixels).unwrap();
    let stride = info.color_type.samples();
    let expected = [color[2], color[1], color[0]];
    let (x, y, width, height) = rect;
    for y in y..y + height {
        for x in x..x + width {
            let offset = (y * info.width as usize + x) * stride;
            let actual = &pixels[offset..offset + 3];
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.abs_diff(expected) <= 2),
                "{context} at ({x},{y}): {actual:?}, expected {expected:?}"
            );
        }
    }
}

#[test]
fn frame_layout_desktop_marks_stay_behind_apps_except_while_the_icon_is_held() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    client.add("desktop-covering-app".into(), (400, 360), (0, 0), APP);
    let app_id = window(&compositor, "desktop-covering-app")["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("move-window {app_id} 0 0"));
    let id = "file:6465736b746f702d6c61796572";
    client.add(
        hearthspace_common::desktop_item_app_id(id),
        (100, 108),
        (0, 0),
        ITEM,
    );
    let home = query(&compositor, "desktop-status")["items"][0].clone();
    let x = home["x"].as_u64().unwrap() as usize;
    let y = home["y"].as_u64().unwrap() as usize;
    action(&compositor, "pointer-motion-abs 1000 700");
    action(&compositor, "layout-tool");
    // The dashed left outline was previously drawn over this opaque app. Keep
    // the sample clear of the separate frame guides and the app's own chrome.
    assert_color(
        &compositor,
        (x - 3, y + 25, 2, 40),
        APP,
        "an untouched desktop icon's dashed outline covered the app",
    );

    let target = json!({"kind":"item","id":id});
    command(&compositor, "select", target.clone(), Value::Null);
    command(
        &compositor,
        "place",
        target.clone(),
        json!({"surface":"canvas","x":250.,"y":160.}),
    );
    assert_color(
        &compositor,
        (247, 185, 2, 40),
        APP,
        "an explicitly placed icon's selected outline covered the app",
    );
    assert_color(
        &compositor,
        (285, 195, 16, 16),
        APP,
        "a later-mapped Canvas icon covered the app's buffer",
    );

    command(
        &compositor,
        "place",
        target,
        json!({"surface":"canvas","x":520.,"y":160.}),
    );
    action(&compositor, "pointer-motion-abs 560 200");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 180 200");
    assert_color(
        &compositor,
        (170, 225, 12, 12),
        ITEM,
        "the actively held icon must remain visible over the app",
    );
    action(&compositor, "pointer-button-up 272");
    assert_color(
        &compositor,
        (137, 185, 2, 40),
        APP,
        "the dropped icon's selected outline must return behind the app",
    );
    assert_color(
        &compositor,
        (170, 225, 12, 12),
        APP,
        "the dropped icon's buffer must return behind the app",
    );
    assert!(client.presses().is_empty());
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
