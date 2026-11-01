use super::*;
use serde_json::json;

pub(super) fn place(compositor: &HeadlessCompositor, id: &str, placement: Value) {
    action(compositor, "canvas-tool layout");
    let status = query(compositor, "layout-status");
    action(
        compositor,
        format!(
            "layout-command {}",
            json!({
                "session": status["session"], "output": status["output"],
                "action": "place", "target": {"kind":"item", "id":id}, "placement":placement
            })
        ),
    );
    action(compositor, "canvas-tool pan");
}

#[test]
fn canvas_pan_on_canvas_icon_matches_reference_without_moving_or_selecting_icon() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = crate::shell_panels::connect(&compositor);
    let id = "file:70616e";
    client.add(
        hearthspace_common::desktop_item_app_id(id),
        (100, 108),
        (0, 0),
        [40, 180, 100, 255],
    );
    place(
        &compositor,
        id,
        json!({"surface":"canvas", "x":5788., "y":3712.}),
    );
    action(&compositor, "pan-to 5760 3600");
    let before = query(&compositor, "desktop-status")["items"].clone();
    start(&compositor, (72., 160.));
    motion(&compositor, (176., 368.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [5656, 3392]);
    assert_eq!(query(&compositor, "desktop-status")["items"], before);
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    assert_eq!(tool(&compositor), "pan");
}

#[test]
fn canvas_pan_excludes_frame_icons_and_shell_controls() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = crate::shell_panels::connect(&compositor);
    let id = "file:6672616d65";
    let item = client.add(
        hearthspace_common::desktop_item_app_id(id),
        (100, 108),
        (0, 0),
        [40, 180, 100, 255],
    );
    place(
        &compositor,
        id,
        json!({"surface":"frame", "edge":"right", "position":0.5}),
    );
    // Match the shell's fixed-size hints after attaching the existing icon.
    // A standalone 54px right-edge item is inset 24px and vertically centered.
    client.resize(item, (54, 54));
    assert_eq!(client.size(item), (54, 54));
    let point = (1229., 400.);
    start(&compositor, point);
    motion(&compositor, (point.0 - 60., point.1 + 40.));
    finish(&compositor);
    assert_eq!(
        offset(&compositor),
        [0, 0],
        "frame icon started a canvas pan"
    );
    let panel = client.add(
        hearthspace_common::TRAY_MENU_APP_ID.into(),
        (250, 150),
        (0, 0),
        [40, 100, 180, 255],
    );
    let status = query(&compositor, "status");
    let bounds = &status["shell_panels"][0];
    let point = (
        bounds["x"].as_f64().unwrap() + 30.,
        bounds["y"].as_f64().unwrap() + 30.,
    );
    start(&compositor, point);
    motion(&compositor, (point.0 - 20., point.1 + 20.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [0, 0]);
    assert_eq!(client.presses().last().unwrap().0, panel);
    assert_eq!(client.releases().last().unwrap().0, panel);
}
