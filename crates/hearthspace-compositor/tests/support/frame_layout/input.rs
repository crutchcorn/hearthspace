//! Consumed backdrop gestures cannot create unmatched native releases.
use super::*;
use hearthspace_common::frame_layout::{layout_confirm_app_id, layout_editor_app_id};

#[test]
fn frame_layout_background_and_modal_capture_all_buttons_without_stealing_native_drags() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    action(&compositor, "layout-tool");
    let state = layout(&compositor);
    let mut client = connect(&compositor);
    let editor = client.add(
        layout_editor_app_id(state["session"].as_u64().unwrap(), state["output"].as_str()),
        (260, 156),
        (0, 0),
        COLOR,
    );
    action(&compositor, "pointer-motion-abs 600 600");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 640 110");
    action(&compositor, "pointer-button-up 272");
    assert!(client.presses().is_empty());
    assert!(client.releases().is_empty());
    super::super::shell_panels::click(&compositor, 640., 110.);
    assert_eq!(client.presses().len(), 1);
    assert_eq!(client.releases().len(), 1);
    assert_eq!(client.presses()[0].0, editor);
    command(
        &compositor,
        "remove",
        json!({"kind":"widget","id":"tools"}),
        Value::Null,
    );
    let state = layout(&compositor);
    assert_eq!(state["confirmation"]["kind"], "hide-tools");
    let modal = client.add(
        layout_confirm_app_id(state["session"].as_u64().unwrap(), state["output"].as_str()),
        (420, 220),
        (0, 0),
        COLOR,
    );
    for button in [272, 273, 274] {
        action(&compositor, "pointer-motion-abs 600 700");
        action(&compositor, format!("pointer-button-down {button}"));
        action(&compositor, "pointer-motion-abs 640 400");
        action(&compositor, format!("pointer-button-up {button}"));
    }
    assert_eq!(client.presses().len(), 1);
    assert_eq!(client.releases().len(), 1);
    action(&compositor, "pointer-motion-abs 640 400");
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 600 700");
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.presses().len(), 2);
    assert_eq!(client.releases().len(), 2);
    assert_eq!(client.releases()[1].0, modal);
    command(&compositor, "cancel", Value::Null, Value::Null);
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
