use serde_json::{Value, json};
use xilem::kurbo::{Rect, Size};

use super::*;

fn status() -> Value {
    json!({
        "viewport": {"x": 200.0, "y": -100.0, "scale": 1.0},
        "outputs": [{"name": "eDP-1", "logical_x": 0, "logical_y": 0,
            "logical_width": 1280, "logical_height": 800, "scale": 2.0}],
        "windows": [{"id": 42, "title": "Notes", "app_id": "org.editor",
            "canvas_x": 300, "canvas_y": 100, "content_width": 800,
            "content_height": 600, "titlebar_height": 32, "focused": true,
            "x": 100, "y": 200, "width": 800, "height": 632,
            "maximized": false, "fullscreen": false}]
    })
}

fn parse(value: Value) -> CanvasSnapshot {
    parse_snapshot(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-7, "{actual} != {expected}");
}

fn assert_inside(rect: Rect, size: Size) {
    assert!(rect.x0 >= 13.0 && rect.y0 >= 13.0, "{rect:?}");
    assert!(
        rect.x1 <= size.width - 13.0 && rect.y1 <= size.height - 13.0,
        "{rect:?}"
    );
}

#[test]
fn preserves_identity_and_unscaled_canvas_geometry() {
    let snapshot = parse(status());
    let window = &snapshot.windows[0];
    assert_eq!(window.id, 42);
    assert_eq!(window.title, "Notes");
    assert_eq!(window.app_id, "org.editor");
    assert!(window.focused);
    assert_eq!(window.bounds, Rect::new(300.0, 100.0, 1100.0, 732.0));
    assert_eq!(snapshot.scale(), 1.0);
    assert_eq!(
        snapshot.output_bounds(&snapshot.outputs[0]),
        Rect::new(200.0, -100.0, 1480.0, 700.0)
    );
}

#[test]
fn zoom_changes_viewport_extent_without_changing_normal_window_size() {
    let first = parse(status());
    let mut value = status();
    value["viewport"]["scale"] = json!(0.5);
    value["windows"][0]["titlebar_height"] = json!(16);
    let zoomed = parse(value);
    assert_eq!(first.windows[0].bounds, zoomed.windows[0].bounds);
    assert_eq!(
        zoomed.output_bounds(&zoomed.outputs[0]),
        Rect::new(200.0, -100.0, 2760.0, 1500.0)
    );
    // Physical monitor scale has already been accounted for in logical bounds.
    assert_close(zoomed.output_bounds(&zoomed.outputs[0]).width(), 2560.0);
}

#[test]
fn projection_includes_negative_and_distant_windows_and_preserves_aspect() {
    let mut value = status();
    let mut distant = value["windows"][0].clone();
    distant["id"] = json!(43);
    distant["canvas_x"] = json!(45000);
    distant["canvas_y"] = json!(-24000);
    value["windows"][0]["canvas_x"] = json!(-30000);
    value["windows"].as_array_mut().unwrap().push(distant);
    let snapshot = parse(value);
    let size = Size::new(312.0, 190.0);
    let map = Projection::new(Some(&snapshot), size);
    for window in &snapshot.windows {
        let projected = map.rect(window.bounds);
        assert_inside(projected, size);
        assert_close(
            projected.width() / projected.height(),
            window.bounds.width() / window.bounds.height(),
        );
    }
    assert_inside(map.rect(snapshot.output_bounds(&snapshot.outputs[0])), size);
    assert!(map.world.x0 <= -30000.0 && map.world.x1 >= 45800.0);
    assert!(map.world.y0 <= -24000.0);
}

#[test]
fn empty_canvas_has_wide_stable_context() {
    let snapshot =
        parse(json!({"viewport": {"x": 0, "y": 0, "scale": 1}, "windows": [], "outputs": []}));
    let size = Size::new(312.0, 190.0);
    let empty = Projection::new(Some(&snapshot), size);
    let loading = Projection::new(None, size);
    assert_eq!(empty.world, loading.world);
    assert!(empty.world.width() >= 8192.0 && empty.world.height() >= 5120.0);
    assert_eq!(empty.point(0.0, 0.0), size.to_rect().center());
    assert!(empty.scale.is_finite() && empty.scale > 0.0);
}

#[test]
fn ordinary_pan_moves_viewport_but_keeps_map_and_windows_still() {
    let initial = parse(status());
    let mut value = status();
    value["viewport"]["x"] = json!(900);
    value["viewport"]["y"] = json!(500);
    let panned = parse(value);
    let size = Size::new(312.0, 190.0);
    let first = Projection::new(Some(&initial), size);
    let second = Projection::new(Some(&panned), size);
    assert_eq!(first.world, second.world);
    assert_eq!(
        first.rect(initial.windows[0].bounds),
        second.rect(panned.windows[0].bounds)
    );
    assert_ne!(
        first.rect(initial.output_bounds(&initial.outputs[0])),
        second.rect(panned.output_bounds(&panned.outputs[0]))
    );
}

#[test]
fn each_output_uses_its_own_logical_origin_at_current_zoom() {
    let mut value = status();
    value["viewport"]["scale"] = json!(2.0);
    value["windows"][0]["titlebar_height"] = json!(64);
    value["outputs"].as_array_mut().unwrap().push(json!({
        "name": "HDMI-A-1", "logical_x": -1920, "logical_y": 300,
        "logical_width": 1920, "logical_height": 1080, "scale": 1.0,
    }));
    let snapshot = parse(value);
    assert_eq!(
        snapshot.output_bounds(&snapshot.outputs[1]),
        Rect::new(-760.0, 50.0, 200.0, 590.0)
    );
    let description = description(Some(&snapshot), Some("HDMI-A-1"));
    assert!(description.contains("on HDMI-A-1"));
    assert!(description.contains("Focused window: Notes"));
}

#[test]
fn presented_windows_and_their_dialogs_use_actual_presentation_location() {
    for field in ["maximized", "fullscreen", "output_coordinates"] {
        let mut value = status();
        value["viewport"]["scale"] = json!(0.5);
        value["windows"][0][field] = json!(true);
        value["windows"][0]["canvas_x"] = json!(999999);
        value["windows"][0]["canvas_y"] = json!(-999999);
        value["windows"][0]["x"] = json!(-1920);
        value["windows"][0]["y"] = json!(46);
        value["windows"][0]["width"] = json!(1920);
        value["windows"][0]["height"] = json!(1034);
        let snapshot = parse(value);
        assert_eq!(
            snapshot.windows[0].bounds,
            Rect::new(-3640.0, -8.0, 200.0, 2060.0)
        );
    }
}

#[test]
fn invalid_json_or_geometry_never_reaches_projection() {
    for bytes in [
        b"not json".as_slice(),
        b"{}",
        br#"{"viewport":{"x":NaN}}"#,
        br#"{"viewport":{"scale":1e400}}"#,
    ] {
        assert!(parse_snapshot(bytes).is_err());
    }
    for invalid in [
        json!(0),
        json!(-1),
        json!(1e-300),
        json!(1e300),
        Value::Null,
    ] {
        let mut value = status();
        value["viewport"]["scale"] = invalid;
        assert!(parse_snapshot(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    for field in ["content_width", "content_height", "id"] {
        let mut value = status();
        value["windows"][0][field] = json!(0);
        assert!(parse_snapshot(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    for path in ["canvas_x", "content_width", "titlebar_height"] {
        let mut value = status();
        value["windows"][0][path] = json!(1e300);
        assert!(parse_snapshot(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut value = status();
    value["outputs"][0]["logical_width"] = json!(-1);
    assert!(parse_snapshot(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn descriptions_cover_loading_empty_and_untitled_windows() {
    assert_eq!(description(None, None), "Canvas minimap loading");
    let mut value = status();
    value["windows"][0]["title"] = Value::Null;
    let accessible_description = description(Some(&parse(value)), None);
    assert!(accessible_description.contains("Focused window: org.editor"));
    assert!(accessible_description.contains("Orange outline shows the visible area"));
    let mut value = status();
    value["windows"] = json!([]);
    assert!(description(Some(&parse(value)), None).contains("0 windows"));
}

#[test]
fn optimistic_pan_keeps_presented_windows_attached_to_their_outputs() {
    let mut value = status();
    let mut presented = value["windows"][0].clone();
    presented["id"] = json!(43);
    presented["output_coordinates"] = json!(true);
    value["windows"].as_array_mut().unwrap().push(presented);
    let mut optimistic = parse(value.clone());
    let normal = optimistic.windows[0].bounds;
    optimistic.set_offset(-400, 900);
    value["viewport"]["x"] = json!(-400);
    value["viewport"]["y"] = json!(900);
    assert_eq!(optimistic, parse(value));
    assert_eq!(optimistic.windows[0].bounds, normal);
}
