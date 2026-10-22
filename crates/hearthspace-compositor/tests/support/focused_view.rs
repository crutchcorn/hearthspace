//! Focused view is compositor presentation, independent of protocol fullscreen.
use super::*;

fn settled(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    id: u64,
    bounds: [i32; 4],
    focused_view: bool,
    fullscreen: bool,
) -> Value {
    client.sync();
    wait_windows(compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == id
                && window["maximized"] == true
                && window["focused_view"] == focused_view
                && window["fullscreen"] == fullscreen
                && ["x", "y", "width", "height"]
                    .into_iter()
                    .zip(bounds)
                    .all(|(field, expected)| window[field] == expected)
        })
    })
    .into_iter()
    .find(|window| window["id"] == id)
    .unwrap()
}

fn assert_original(restored: &Value, original: &Value) {
    assert_eq!(restored["maximized"], false);
    assert_eq!(restored["focused_view"], false);
    assert_eq!(restored["fullscreen"], false);
    for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
        assert_eq!(restored[field], original[field], "restore {field}");
    }
}

#[test]
fn focused_view_keyboard_entry_clears_window_and_desktop_selections() {
    use super::super::navigation::keys;
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(1);
    let other = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|window| window["id"].as_u64().filter(|other| *other != id))
        .unwrap();
    let mut desktop = super::super::shell_panels::connect(&compositor);
    desktop.add(
        hearthspace_common::desktop_item_app_id("app:selection.desktop"),
        (100, 108),
        (0, 0),
        [0, 255, 0, 255],
    );
    action(&compositor, "desktop-select \"app:selection.desktop\"");
    action(&compositor, format!("focus-window {other}"));
    action(&compositor, "group-selection-toggle");
    keys(&compositor, &[57]); // Select the other window with Space.
    keys(&compositor, &[28]); // Retain the selection after leaving selection mode.
    action(&compositor, format!("focus-window {id}"));
    assert_eq!(
        query(&compositor, "group-status")["selected"],
        serde_json::json!([other])
    );
    assert_eq!(
        query(&compositor, "desktop-status")["items"][0]["selected"],
        true
    );

    // Keyboard entry does not have the outside pointer press which ordinarily
    // dismisses these selections. Focused view must clear them itself.
    keys(&compositor, &[125, 42, 33]); // Super+Shift+F.
    settled(&compositor, &mut client, id, [0, 0, 1280, 800], true, false);
    let group = query(&compositor, "group-status");
    assert_eq!(group["selected"], serde_json::json!([]));
    assert_eq!(group["selecting"], false);
    let desktop = query(&compositor, "desktop-status");
    assert_eq!(desktop["items"][0]["selected"], false);
    assert_eq!(desktop["items"][0]["keyboard_focus"], false);
    assert_eq!(desktop["select_mode"], false);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn focused_view_retains_header_and_restores_through_bounded_maximize_at_both_scales() {
    let _guard = headless_test_guard();
    for (width, height, scale) in [(1280, 800, 1), (1280, 800, 2), (760, 600, 2)] {
        let (mut compositor, mut client, id) = fixture_with_size(scale, width, height);
        let original = window(&compositor, id);
        let (side, vertical) = if width <= 760 { (12, 78) } else { (24, 96) };
        let bounded = [side, vertical, width - 2 * side, height - 2 * vertical];
        let output = [0, 0, width, height];

        double_click_title(&compositor, &original);
        let maximized = settled(&compositor, &mut client, id, bounded, false, false);
        assert_eq!(maximized["titlebar_height"], 44);
        assert_eq!(maximized["focused"], true);

        click(&compositor, button_point(&maximized, true));
        let focused = settled(&compositor, &mut client, id, output, true, false);
        assert_eq!(focused["titlebar_height"], 44);
        assert_eq!(focused["content_height"], height - 44);
        assert_eq!(focused["focused"], true);

        click(&compositor, button_point(&focused, true));
        let maximized = settled(&compositor, &mut client, id, bounded, false, false);
        // A second focused-view visit must retain the original floating size,
        // rather than treating the already maximized size as its restore size.
        click(&compositor, button_point(&maximized, true));
        let focused = settled(&compositor, &mut client, id, output, true, false);
        double_click_title(&compositor, &focused);
        let maximized = settled(&compositor, &mut client, id, bounded, false, false);
        double_click_title(&compositor, &maximized);
        let restored = assert_presentation(&compositor, &mut client, id, false);
        assert_original(&restored, &original);
        assert!(
            !client.received_pointer_press(),
            "chrome presses reached application content"
        );
        assert!(
            client.released_buttons().is_empty(),
            "chrome leaked a release"
        );
        assert!(client.closed_windows().is_empty());
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
}

#[test]
fn focused_view_rapid_toggles_before_client_resize_preserve_original_geometry() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(1);
    let original = window(&compositor, id);

    // Do not dispatch any client events during these transitions. The server
    // must preserve restore state through a sequence of uncommitted configures.
    click(&compositor, button_point(&original, true));
    let focused = window(&compositor, id);
    assert_eq!(focused["focused_view"], true);
    assert_eq!(focused["content_width"], original["content_width"]);
    click(&compositor, button_point(&focused, true));
    let bounded = window(&compositor, id);
    assert_eq!(bounded["focused_view"], false);
    assert_eq!(bounded["maximized"], true);
    click(&compositor, button_point(&bounded, true));
    let focused = window(&compositor, id);
    assert_eq!(focused["focused_view"], true);
    double_click_title(&compositor, &focused);
    let bounded = window(&compositor, id);
    assert_eq!(bounded["focused_view"], false);
    assert_eq!(bounded["maximized"], true);
    double_click_title(&compositor, &bounded);

    let restored = assert_presentation(&compositor, &mut client, id, false);
    assert_original(&restored, &original);
    assert!(!client.received_pointer_press());
    assert!(client.released_buttons().is_empty());
    assert!(client.closed_windows().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn protocol_fullscreen_temporarily_hides_header_without_discarding_focused_view() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(1);
    let original = window(&compositor, id);
    click(&compositor, button_point(&original, true));
    let focused = settled(&compositor, &mut client, id, [0, 0, 1280, 800], true, false);
    assert_eq!(focused["titlebar_height"], 44);

    client.request_fullscreen(0);
    let fullscreen = settled(&compositor, &mut client, id, [0, 0, 1280, 800], true, true);
    assert_eq!(fullscreen["titlebar_height"], 0);
    assert_eq!(fullscreen["content_height"], 800);
    action(&compositor, format!("fullscreen-window {id} false"));
    let focused = settled(&compositor, &mut client, id, [0, 0, 1280, 800], true, false);
    assert_eq!(focused["titlebar_height"], 44);
    assert_eq!(focused["content_height"], 756);

    action(&compositor, format!("restore-window {id}"));
    let restored = assert_presentation(&compositor, &mut client, id, false);
    assert_original(&restored, &original);
    assert!(!client.received_pointer_press());
    assert!(client.released_buttons().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn header_maximize_fits_current_zoom_then_tracks_canvas_pan_and_zoom_until_restored() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture(2);
    action(&compositor, "zoom-out");
    super::super::canvas_zoom::wait_scale(&compositor, 0.8);
    let original = window(&compositor, id);
    double_click_title(&compositor, &original);
    let maximized = settled(
        &compositor,
        &mut client,
        id,
        [24, 96, 1232, 608],
        false,
        false,
    );
    assert_eq!(maximized["titlebar_height"], 35);
    assert_eq!(maximized["content_width"], 1540);
    assert_eq!(maximized["content_height"], 716);

    // Navigation changes the visible projection, never the maximized client
    // allocation or the saved floating home. The output scale remains 2x.
    action(&compositor, "pan-to -60 -150");
    let panned = settled(
        &compositor,
        &mut client,
        id,
        [-56, 136, 1232, 608],
        false,
        false,
    );
    for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
        assert_eq!(panned[field], maximized[field], "pan changed {field}");
    }
    action(&compositor, "zoom-out");
    super::super::canvas_zoom::wait_scale(&compositor, 0.64);
    let zoomed = settled(
        &compositor,
        &mut client,
        id,
        [83, 189, 986, 486],
        false,
        false,
    );
    assert_eq!(zoomed["titlebar_height"], 28);
    for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
        assert_eq!(zoomed[field], maximized[field], "zoom changed {field}");
    }
    double_click_title(&compositor, &zoomed);
    let restored = assert_presentation(&compositor, &mut client, id, false);
    assert_original(&restored, &original);
    assert!(!client.received_pointer_press());
    assert!(client.released_buttons().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
