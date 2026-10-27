use masonry::{
    core::{PointerButtonEvent, PointerInfo, PointerState, PointerType, PointerUpdate},
    testing::TestHarness,
};
use serde_json::json;

use super::*;
use crate::ui::minimap::parse_snapshot;

fn snapshot(scale: f64) -> CanvasSnapshot {
    parse_snapshot(
        &serde_json::to_vec(&json!({
            "viewport": {"x": -200, "y": 300, "scale": scale},
            "outputs": [{"name":"eDP-1", "logical_x":0, "logical_y":0,
                "logical_width":1280, "logical_height":800},
                {"name":"HDMI-A-1", "logical_x":-1920, "logical_y":120,
                "logical_width":1920, "logical_height":1080}],
            "windows": [{"id":1,"canvas_x":800,"canvas_y":700,
                "content_width":600,"content_height":400,"titlebar_height":0}]
        }))
        .unwrap(),
    )
    .unwrap()
}

fn harness(scale: f64) -> TestHarness<Minimap> {
    TestHarness::create_with_size(
        crate::ui::appearance::properties(hearthspace_ui_components::Palette::default()),
        Minimap::new(Some(snapshot(scale)), Some("HDMI-A-1".into())).prepare(),
        (310, 168),
    )
}

fn center(harness: &TestHarness<Minimap>) -> Point {
    let widget = harness.root_widget();
    let map = Projection::new(widget.snapshot.as_ref(), widget.size);
    widget.viewport(&map).unwrap().center()
}

fn drain(harness: &mut TestHarness<Minimap>) -> Vec<MapAction> {
    std::iter::from_fn(|| harness.pop_action::<MapAction>().map(|(action, _)| action)).collect()
}

#[test]
fn mouse_drag_uses_frozen_map_scale_and_leaves_windows_and_zoom_unchanged() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut harness = harness(zoom);
        let start = center(&harness);
        let before = harness.root_widget().snapshot.clone().unwrap();
        let map = Projection::new(Some(&before), Size::new(310.0, 168.0));
        harness.mouse_move(start);
        harness.mouse_button_press(Some(PointerButton::Primary));
        assert_eq!(drain(&mut harness), [MapAction::Begin]);
        let destination = start + (44.0, -32.0);
        harness.mouse_move(destination);
        let expected = (
            (before.offset.0 + 44.0 / map.scale).round() as i32,
            (before.offset.1 - 32.0 / map.scale).round() as i32,
        );
        assert_eq!(
            drain(&mut harness),
            [MapAction::PanTo {
                x: expected.0,
                y: expected.1
            }]
        );
        assert_eq!(
            harness
                .root_widget()
                .drag
                .as_ref()
                .unwrap()
                .projection
                .world,
            map.world
        );
        let after = harness.root_widget().snapshot.clone().unwrap();
        assert_eq!(after.scale(), zoom);
        assert_eq!(after.windows, before.windows);
        assert_eq!(after.offset, (f64::from(expected.0), f64::from(expected.1)));
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert_eq!(drain(&mut harness), [MapAction::End]);
        harness.mouse_move(start);
        assert!(drain(&mut harness).is_empty());
        assert!(harness.root_widget().drag.is_none());
    }
}

fn touch_event(kind: &str, point: Point) -> PointerEvent {
    let pointer = PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Touch,
    };
    let state = PointerState {
        position: (point.x, point.y).into(),
        pressure: 0.5,
        ..PointerState::default()
    };
    match kind {
        "down" => PointerEvent::Down(PointerButtonEvent {
            button: None,
            pointer,
            state,
        }),
        "up" => PointerEvent::Up(PointerButtonEvent {
            button: None,
            pointer,
            state,
        }),
        "move" => PointerEvent::Move(PointerUpdate {
            pointer,
            current: state,
            coalesced: vec![],
            predicted: vec![],
        }),
        "cancel" => PointerEvent::Cancel(pointer),
        _ => unreachable!(),
    }
}

#[test]
fn touch_drag_cancels_cleanly_and_next_contact_can_drag() {
    let mut harness = harness(1.0);
    let start = center(&harness);
    harness.process_pointer_event(touch_event("down", start));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    harness.process_pointer_event(touch_event("move", start + (15.0, 20.0)));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::PanTo { .. }]
    ));
    let stopped = harness.root_widget().snapshot.as_ref().unwrap().offset;
    harness.process_pointer_event(touch_event("cancel", start));
    assert_eq!(drain(&mut harness), [MapAction::End]);
    harness.process_pointer_event(touch_event("move", start + (80.0, 50.0)));
    assert!(drain(&mut harness).is_empty());
    assert_eq!(
        harness.root_widget().snapshot.as_ref().unwrap().offset,
        stopped
    );
    let new_start = center(&harness);
    harness.process_pointer_event(touch_event("down", new_start));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    // The final contact position is applied even without a preceding move.
    harness.process_pointer_event(touch_event("up", new_start + (-10.0, -10.0)));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::PanTo { .. }, MapAction::End]
    ));
}

#[test]
fn focus_loss_stops_drag_without_applying_another_position() {
    let mut harness = harness(1.0);
    let start = center(&harness);
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    harness.process_text_event(TextEvent::WindowFocusChange(false));
    assert_eq!(drain(&mut harness), [MapAction::End]);
    harness.mouse_move(start + (10.0, 0.0));
    assert!(drain(&mut harness).is_empty());
    assert!(harness.root_widget().drag.is_none());
}

#[test]
fn movement_clamps_to_compositor_bounds_and_projection_stays_frozen() {
    let mut widget = Minimap::new(Some(snapshot(1.0)), Some("HDMI-A-1".into()));
    widget.size = Size::new(310.0, 168.0);
    let map = Projection::new(widget.snapshot.as_ref(), widget.size);
    let start = widget.viewport(&map).unwrap().center();
    assert!(widget.begin(start, Some(PointerId::PRIMARY)));
    assert_eq!(
        widget.pan(start + (1e9, -1e9)),
        Some(MapAction::PanTo {
            x: 1_000_000,
            y: -1_000_000
        })
    );
    assert_eq!(widget.drag.as_ref().unwrap().projection.world, map.world);
    assert_eq!(
        widget.pan(start),
        Some(MapAction::PanTo { x: -200, y: 300 })
    );
    assert_eq!(widget.pan(start), None);
}

#[test]
fn secondary_release_cannot_leave_a_drag_running_after_capture_is_lost() {
    let mut harness = harness(1.0);
    let start = center(&harness);
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    harness.mouse_button_press(Some(PointerButton::Secondary));
    harness.mouse_button_release(Some(PointerButton::Secondary));
    assert_eq!(drain(&mut harness), [MapAction::End]);
    harness.mouse_move(start + (20.0, 20.0));
    assert!(drain(&mut harness).is_empty());
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert!(harness.root_widget().drag.is_none());
}

#[path = "widget_tests/appearance.rs"]
mod appearance;
#[path = "widget_tests/interaction.rs"]
mod interaction;
