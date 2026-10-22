//! Canvas maximization keeps stable reference clearances as frame controls change.
use super::*;
use hearthspace_common::frame_layout::{frame_group_app_id, frame_widget_app_id};

fn maximized_bounds(compositor: &HeadlessCompositor) {
    let window = window(compositor, "frame-work-area");
    assert_eq!(window["x"], 24, "{window}");
    assert_eq!(window["y"], 96, "{window}");
    assert_eq!(window["width"], 752, "{window}");
    assert_eq!(window["height"], 408, "{window}");
}

#[test]
fn frame_layout_changes_preserve_the_maximized_canvas_bounds() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("800x600");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let normal = client.add("frame-work-area".into(), (240, 160), (0, 0), COLOR);
    let id = window(&compositor, "frame-work-area")["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("maximize-window {id} true"));
    client.sync();
    maximized_bounds(&compositor);

    client.add(
        frame_widget_app_id(Some("unplugged"), "wifi"),
        (44, 44),
        (0, 0),
        COLOR,
    );
    maximized_bounds(&compositor);
    let wifi = client.add(
        frame_widget_app_id(Some(&output), "wifi"),
        (44, 44),
        (0, 0),
        COLOR,
    );
    maximized_bounds(&compositor);
    assert_eq!(client.size(normal), (752, 408));

    // Customized placement and visibility must not resize an existing maximized app.
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "place",
        json!({"kind":"widget","id":"wifi"}),
        json!({"surface":"canvas","x":350.,"y":250.}),
    );
    maximized_bounds(&compositor);
    command(
        &compositor,
        "remove",
        json!({"kind":"widget","id":"wifi"}),
        Value::Null,
    );
    command(&compositor, "done", Value::Null, Value::Null);
    maximized_bounds(&compositor);
    client.destroy(wifi);
    maximized_bounds(&compositor);
    assert_eq!(client.size(normal), (752, 408));

    let group = client.add(
        frame_group_app_id(Some(&output), "system"),
        (300, 54),
        (0, 0),
        COLOR,
    );
    maximized_bounds(&compositor);
    assert_eq!(client.size(normal), (752, 408));
    client.destroy(group);
    maximized_bounds(&compositor);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
