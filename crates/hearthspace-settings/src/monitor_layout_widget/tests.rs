use super::*;
use hearthspace_ipc::Settings;
use xilem::masonry::{
    core::{
        PointerInfo, PointerType, WindowEvent,
        keyboard::{Key, NamedKey},
    },
    testing::TestHarness,
};

fn assert_inside(widget: &MonitorLayoutWidget, size: Size) {
    let metrics = widget.layout_metrics(size);
    for monitor in &widget.monitors {
        let rect = metrics.rect(monitor);
        assert!(
            rect.x0 >= 0.0 && rect.y0 >= 0.0 && rect.x1 <= size.width && rect.y1 <= size.height,
            "{} escapes {size:?}: {rect:?}",
            monitor.id
        );
        assert!(widget.hit_monitor(rect.center(), metrics).is_some());
        let center = metrics.world_point(rect.center());
        assert!(
            (center.x - (f64::from(monitor.x) + f64::from(monitor_logical_width(monitor)) / 2.0))
                .abs()
                < 1e-6
        );
    }
    assert!(
        widget
            .hit_monitor(Point::new(-1.0, size.height / 2.0), metrics)
            .is_none()
    );
    assert!(
        widget
            .hit_monitor(Point::new(size.width + 1.0, size.height / 2.0), metrics)
            .is_none()
    );
}

#[test]
fn negative_separated_large_monitors_fit_without_changing_desktop_coordinates() {
    let mut monitors = Settings::default().monitors;
    monitors[0].x = -12_000;
    monitors[0].y = -8_000;
    monitors[0].width = 5120;
    monitors[1].x = 8_000;
    monitors[1].y = -2_000;
    monitors[1].scale = 2;
    let widget = MonitorLayoutWidget::new(monitors.clone(), Some("primary".into()));
    for size in [
        Size::new(400.0, 225.0),
        Size::new(180.0, 90.0),
        Size::new(700.0, 130.0),
    ] {
        assert_inside(&widget, size);
    }
    assert_eq!(widget.monitors, monitors);
}

#[test]
fn captured_drag_far_outside_and_preview_resize_keep_tiles_inside_until_release() {
    let monitors = Settings::default().monitors;
    let original_primary = monitors[0].clone();
    let widget = MonitorLayoutWidget::new(monitors, Some("secondary".into()));
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_light_properties(),
        widget.prepare(),
        (400, 225),
    );
    let size = Size::new(400.0, 225.0);
    let start = {
        let widget = harness.root_widget();
        widget
            .layout_metrics(size)
            .rect(&widget.monitors[1])
            .center()
    };
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    let canvas = harness.root_widget().drag.as_ref().unwrap().canvas;
    harness.mouse_move((100_000.0, -100_000.0));
    assert_eq!(harness.root_widget().monitors[0], original_primary);
    assert_eq!(harness.root_widget().drag.as_ref().unwrap().canvas, canvas);
    assert_inside(&harness.root_widget(), size);
    harness.process_window_event(WindowEvent::Resize((240, 140).into()));
    let resized = Size::new(240.0, 140.0);
    assert_inside(&harness.root_widget(), resized);
    harness.mouse_move((-100_000.0, 100_000.0));
    assert_inside(&harness.root_widget(), resized);
    let before = harness.root_widget().monitors.clone();
    let expected = snap_monitor_to_neighbor(&before, "secondary", before[1].x, before[1].y);
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert!(harness.root_widget().drag.is_none());
    assert_inside(&harness.root_widget(), resized);
    let after = harness.root_widget().monitors.clone();
    assert_eq!(
        (expected.0 - before[0].x, expected.1 - before[0].y),
        (after[1].x - after[0].x, after[1].y - after[0].y)
    );
    assert!(!crate::monitors::monitors_overlap(&after));
    let mut committed = 0;
    while let Some((action, _)) = harness.pop_action::<MonitorEditorAction>() {
        if matches!(action, MonitorEditorAction::Drag { commit: true, .. }) {
            committed += 1;
        }
    }
    assert_eq!(
        committed, 1,
        "only release commits the bounded native arrangement"
    );
}

#[test]
fn clicking_a_monitor_selects_it_without_moving_or_saving_the_arrangement() {
    let monitors = Settings::default().monitors;
    let widget = MonitorLayoutWidget::new(monitors.clone(), Some("primary".into()));
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_light_properties(),
        widget.prepare(),
        (400, 225),
    );
    let start = harness
        .root_widget()
        .layout_metrics(Size::new(400.0, 225.0))
        .rect(&monitors[1])
        .center();
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    // Normal hand jitter within the click threshold must remain a selection.
    harness.mouse_move(start + Vec2::new(1.0, 1.0));
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert_eq!(
        harness.root_widget().selected_monitor_id.as_deref(),
        Some("secondary")
    );
    assert_eq!(harness.root_widget().monitors, monitors);
    assert!(harness.root_widget().drag.is_none());
    assert_eq!(harness.pointer_capture_target_id(), None);
    let mut selections = 0;
    while let Some((action, _)) = harness.pop_action::<MonitorEditorAction>() {
        let MonitorEditorAction::Select {
            id,
            monitors: selected,
        } = action
        else {
            panic!("a click must never request a live or persisted position change");
        };
        assert_eq!(id, "secondary");
        assert_eq!(selected, monitors);
        selections += 1;
    }
    assert!(selections > 0);
}

#[test]
fn losing_pointer_capture_restores_the_original_layout_without_saving_on_release() {
    let monitors = Settings::default().monitors;
    let widget = MonitorLayoutWidget::new(monitors.clone(), Some("secondary".into()));
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_light_properties(),
        widget.prepare(),
        (400, 225),
    );
    let start = harness
        .root_widget()
        .layout_metrics(Size::new(400.0, 225.0))
        .rect(&monitors[1])
        .center();
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(start + Vec2::new(-40.0, 50.0));
    assert_ne!(harness.root_widget().monitors, monitors);
    assert_eq!(harness.pointer_capture_target_id(), Some(harness.root_id()));
    harness.process_pointer_event(PointerEvent::Cancel(PointerInfo {
        pointer_id: None,
        persistent_device_id: None,
        pointer_type: PointerType::default(),
    }));
    assert_eq!(harness.pointer_capture_target_id(), None);
    assert!(harness.root_widget().drag.is_none());
    assert_eq!(harness.root_widget().monitors, monitors);
    let mut restored = false;
    while let Some((action, _)) = harness.pop_action::<MonitorEditorAction>() {
        match action {
            MonitorEditorAction::Select { monitors: live, .. } => {
                restored = live == monitors;
            }
            MonitorEditorAction::Drag { commit, .. } => {
                assert!(!commit, "cancelled movement must never be saved");
                restored = false;
            }
        }
    }
    assert!(restored, "the final action restores the original app state");
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert!(harness.pop_action::<MonitorEditorAction>().is_none());
    assert_eq!(harness.root_widget().monitors, monitors);
}

#[test]
fn live_projection_is_stable_and_drag_grab_point_maps_to_original_monitor_coordinates() {
    let mut monitors = Settings::default().monitors;
    monitors[1].width = 3840;
    monitors[1].height = 2160;
    monitors[1].scale = 2;
    let mut widget = MonitorLayoutWidget::new(monitors.clone(), None);
    let size = Size::new(400.0, 225.0);
    let metrics = widget.layout_metrics(size);
    let primary = metrics.rect(&monitors[0]).size();
    let secondary = metrics.rect(&monitors[1]).size();
    assert!(
        (primary.width - secondary.width).abs() < 1e-6
            && (primary.height - secondary.height).abs() < 1e-6,
        "a 4K display at 2× must have the same logical footprint as 1080p at 1×"
    );
    let start = metrics.rect(&monitors[1]).center();
    let drag = MonitorDrag {
        id: "secondary".into(),
        start_pointer: start,
        grab_offset: metrics.world_point(start) - Point::new(1920.0, 0.0),
        canvas: metrics.canvas,
        original: monitors.clone(),
        moved: true,
    };
    assert!((drag.grab_offset - Vec2::new(960.0, 540.0)).hypot() < 1e-6);
    assert_eq!(
        widget.monitors_for_drag(&drag, start, size, false),
        monitors
    );
    widget.drag = Some(drag.clone());
    widget.monitors =
        widget.monitors_for_drag(&drag, Point::new(start.x, start.y + 90.0), size, false);
    let live = widget.layout_metrics(size);
    assert_eq!(live.canvas, metrics.canvas);
    assert_eq!(live.scale, metrics.scale);
    assert_inside(&widget, size);
}

#[test]
fn keyboard_selection_and_arrangement_are_available_with_a_visible_focus_ring() {
    let widget = MonitorLayoutWidget::new(Settings::default().monitors, Some("primary".into()));
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_light_properties(),
        widget.prepare(),
        (400, 225),
    );
    let unfocused = harness.render();
    harness.focus_on(Some(harness.root_id()));
    assert_ne!(
        unfocused,
        harness.render(),
        "focus must have a visible outline"
    );
    harness.process_text_event(TextEvent::key_down(Key::Character(" ".into())));
    assert_eq!(
        harness.root_widget().selected_monitor_id.as_deref(),
        Some("secondary")
    );
    harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowUp)));
    let widget = harness.root_widget();
    let monitors = &widget.monitors;
    assert_eq!(
        monitors[1].y + monitor_logical_height(&monitors[1]),
        monitors[0].y
    );
    assert_eq!(monitors[1].x, monitors[0].x);
    assert_inside(&harness.root_widget(), Size::new(400.0, 225.0));
}

#[test]
fn clicking_a_monitor_keeps_selection_without_a_keyboard_outline() {
    let monitors = Settings::default().monitors;
    let center = MonitorLayoutWidget::new(monitors.clone(), Some("primary".into()))
        .layout_metrics(Size::new(400.0, 225.0))
        .rect(&monitors[0])
        .center();
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_light_properties(),
        MonitorLayoutWidget::new(monitors, Some("primary".into())).prepare(),
        (400, 225),
    );
    harness.mouse_move((1.0, 1.0));
    let selected = harness.render();
    harness.mouse_move(center);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    harness.mouse_move((1.0, 1.0));
    assert_eq!(harness.focused_widget_id(), Some(harness.root_id()));
    assert_eq!(selected, harness.render());
    harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Shift)));
    assert_ne!(selected, harness.render());
}

#[test]
fn hovering_an_unselected_monitor_highlights_it_without_selecting_or_saving() {
    let monitors = Settings::default().monitors;
    let widget = MonitorLayoutWidget::new(monitors.clone(), Some("primary".into()));
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_light_properties(),
        widget.prepare(),
        (400, 225),
    );
    harness.mouse_move((1.0, 1.0));
    let idle = harness.render();
    let second = harness
        .root_widget()
        .layout_metrics(Size::new(400.0, 225.0))
        .rect(&monitors[1])
        .center();
    harness.mouse_move(second);
    assert_ne!(
        idle,
        harness.render(),
        "hover must visibly identify the draggable tile"
    );
    assert_eq!(
        harness.root_widget().selected_monitor_id.as_deref(),
        Some("primary")
    );
    assert_eq!(harness.root_widget().monitors, monitors);
    assert!(harness.pop_action::<MonitorEditorAction>().is_none());
    harness.mouse_move((1.0, 1.0));
    assert_eq!(
        idle,
        harness.render(),
        "moving away must clear the hover treatment"
    );
}

#[test]
fn normalization_keeps_the_relative_native_layout_and_grid_rounding_handles_negative_values() {
    let mut monitors = Settings::default().monitors;
    set_monitor_position(&mut monitors, "primary", 10_000, -8_000);
    set_monitor_position(&mut monitors, "secondary", 11_920, -8_000);
    normalize_monitors_near_origin(&mut monitors);
    assert_eq!(
        monitor_arrangement_bounds(&monitors),
        Some((-1920, -540, 1920, 540))
    );
    assert_eq!(snap_position(15), 20);
    assert_eq!(snap_position(-15), -20);
}

#[test]
fn extreme_monitor_origins_and_dimensions_do_not_overflow_or_distort_normalization() {
    let mut monitors = Settings::default().monitors;
    monitors[0].x = i32::MIN;
    monitors[0].y = i32::MIN;
    monitors[0].width = i32::MAX;
    monitors[0].height = i32::MAX;
    monitors[0].scale = 4;
    monitors[1].x = i32::MAX;
    monitors[1].y = i32::MAX;
    monitors[1].width = i32::MAX;
    monitors[1].height = i32::MAX;
    assert_eq!(monitor_logical_width(&monitors[0]), 536_870_912);
    let difference = (
        i64::from(monitors[1].x) - i64::from(monitors[0].x),
        i64::from(monitors[1].y) - i64::from(monitors[0].y),
    );
    let size = Size::new(400.0, 225.0);
    let widget = MonitorLayoutWidget::new(monitors.clone(), None);
    for monitor in &monitors {
        let rect = widget.layout_metrics(size).rect(monitor);
        assert!(
            rect.x0 >= 0.0 && rect.x1 <= size.width && rect.y0 >= 0.0 && rect.y1 <= size.height
        );
    }
    let metrics = widget.layout_metrics(size);
    let point = metrics.bounded_position(&monitors, "secondary", i32::MIN, i32::MAX);
    let _snapped = snap_monitor_to_neighbor(&monitors, "secondary", point.0, point.1);
    normalize_monitors_near_origin(&mut monitors);
    assert_eq!(
        (
            i64::from(monitors[1].x) - i64::from(monitors[0].x),
            i64::from(monitors[1].y) - i64::from(monitors[0].y)
        ),
        difference
    );
}
