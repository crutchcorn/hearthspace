use super::*;
use hearthspace_ipc::Settings;
use masonry::{
    core::{PointerInfo, PointerType, WindowEvent},
    testing::TestHarness,
};

fn assert_tiles_inside(widget: &MonitorLayoutWidget, size: Size) {
    let metrics = widget.layout_metrics(size);
    for monitor in &widget.monitors {
        let rect = metrics.rect(monitor);
        assert!(
            rect.x0 >= 2.0 - 1e-6
                && rect.y0 >= 2.0 - 1e-6
                && rect.x1 <= size.width - 2.0 + 1e-6
                && rect.y1 <= size.height - 2.0 + 1e-6,
            "tile escapes the visible board: {rect:?} in {size:?}"
        );
        assert!(widget.hit_monitor(rect.center(), metrics).is_some());
    }
}

fn exercise_all_edges_and_resizes(monitors: Vec<MonitorConfig>, selected: usize) {
    let widget = MonitorLayoutWidget::new(monitors.clone(), Some(monitors[selected].id.clone()));
    let mut harness = TestHarness::create_with_size(
        components::hearthspace_light_properties(),
        widget.prepare(),
        (400, 225),
    );
    let start = harness
        .root_widget()
        .layout_metrics(Size::new(400.0, 225.0))
        .rect(&monitors[selected])
        .center();
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    for (width, height) in [(400, 225), (180, 320), (500, 160), (400, 225)] {
        harness.process_window_event(WindowEvent::Resize((width, height).into()));
        let size = Size::new(f64::from(width), f64::from(height));
        assert_tiles_inside(&harness.root_widget(), size);
        for (edge, point) in [
            ("left", Point::new(-100_000.0, size.height / 2.0)),
            ("right", Point::new(100_000.0, size.height / 2.0)),
            ("top", Point::new(size.width / 2.0, -100_000.0)),
            ("bottom", Point::new(size.width / 2.0, 100_000.0)),
        ] {
            harness.mouse_move(point);
            let widget = harness.root_widget();
            assert_tiles_inside(&widget, size);
            let rect = widget.layout_metrics(size).rect(&widget.monitors[selected]);
            let gap = match edge {
                "left" => rect.x0 - 2.0,
                "right" => size.width - 2.0 - rect.x1,
                "top" => rect.y0 - 2.0,
                "bottom" => size.height - 2.0 - rect.y1,
                _ => unreachable!(),
            };
            assert!(
                (-1e-6..=1.0).contains(&gap),
                "unreachable {edge} margin: {gap}px at {size:?}"
            );
            for (index, original) in monitors.iter().enumerate() {
                if index != selected {
                    assert_eq!(&widget.monitors[index], original);
                }
            }
        }
    }
    harness.process_pointer_event(PointerEvent::Cancel(PointerInfo {
        pointer_id: None,
        persistent_device_id: None,
        pointer_type: PointerType::default(),
    }));
    assert_eq!(harness.root_widget().monitors, monitors);
    while let Some((action, _)) = harness.pop_action::<MonitorEditorAction>() {
        assert!(!matches!(
            action,
            MonitorEditorAction::Drag { commit: true, .. }
        ));
    }
}

#[test]
fn a_single_tall_display_can_reach_every_visible_preview_edge_after_resizing() {
    let mut monitors = Settings::default().monitors;
    monitors.truncate(1);
    monitors[0].height = 1200;
    exercise_all_edges_and_resizes(monitors, 0);
}

#[test]
fn scaled_displays_at_negative_coordinates_use_the_full_preview_without_moving_neighbors() {
    let mut monitors = Settings::default().monitors;
    monitors[0].x = -1920;
    monitors[0].y = -1080;
    monitors[1].x = 0;
    monitors[1].y = -1080;
    monitors[1].width = 3840;
    monitors[1].height = 2160;
    monitors[1].scale = 2;
    exercise_all_edges_and_resizes(monitors, 1);
}
