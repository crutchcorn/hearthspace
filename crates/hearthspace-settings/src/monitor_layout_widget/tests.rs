use super::*;
use hearthspace_ipc::Settings;

fn offset_monitor(monitors: &mut [MonitorConfig], id: &str, x: i32, y: i32) {
    set_monitor_position(monitors, id, x, y);
}

#[test]
fn monitor_canvas_size_uses_combined_dimensions_with_multiplier() {
    let monitors = vec![
        MonitorConfig {
            id: "left".to_string(),
            name: "Left".to_string(),
            width: 1280,
            height: 720,
            scale: 1,
            x: -1280,
            y: 120,
        },
        MonitorConfig {
            id: "main".to_string(),
            name: "Main".to_string(),
            width: 1920,
            height: 1080,
            scale: 1,
            x: 0,
            y: 0,
        },
    ];

    assert_eq!(combined_monitor_width(&monitors), 3200);
    assert_eq!(combined_monitor_height(&monitors), 1800);
    assert_eq!(
        monitor_canvas_size(&monitors),
        Size::new(
            3200.0 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
            1800.0 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
        )
    );
}

#[test]
fn monitor_canvas_size_does_not_cap_large_monitors() {
    let monitors = vec![
        MonitorConfig {
            id: "ultrawide".to_string(),
            name: "Ultrawide".to_string(),
            width: 5120,
            height: 1440,
            scale: 1,
            x: 0,
            y: 0,
        },
        MonitorConfig {
            id: "hidpi".to_string(),
            name: "HiDPI".to_string(),
            width: 3840,
            height: 2160,
            scale: 1,
            x: 5120,
            y: 0,
        },
    ];

    assert_eq!(combined_monitor_width(&monitors), 8960);
    assert_eq!(combined_monitor_height(&monitors), 3600);
    assert_eq!(
        monitor_canvas_size(&monitors),
        Size::new(
            8960.0 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
            3600.0 * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
        )
    );
}

#[test]
fn monitor_canvas_size_uses_default_only_when_no_monitors_exist() {
    assert_eq!(
        monitor_canvas_size(&[]),
        Size::new(
            f64::from(EMPTY_MONITOR_CANVAS_WIDTH) * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
            f64::from(EMPTY_MONITOR_CANVAS_HEIGHT) * MONITOR_LAYOUT_CANVAS_MULTIPLIER,
        )
    );
}

#[test]
fn layout_scale_does_not_change_when_monitors_are_dragged() {
    let mut monitors = Settings::default().monitors;
    let size = Size::new(760.0, 320.0);
    let scale = layout_scale_for_canvas(&monitors, size);

    offset_monitor(&mut monitors, "secondary", -3200, 900);

    assert_eq!(layout_scale_for_canvas(&monitors, size), scale);
}

#[test]
fn layout_metrics_do_not_recenter_during_live_drag() {
    let mut widget = MonitorLayoutWidget::new(Settings::default().monitors, None);
    let size = Size::new(760.0, 320.0);
    let metrics = widget.layout_metrics(size);
    widget.drag = Some(MonitorDrag {
        id: "secondary".to_string(),
        start_pointer: Point::from((0.0, 0.0)),
        start_x: 1920,
        start_y: 0,
        origin_x: metrics.origin_x,
        origin_y: metrics.origin_y,
        scale: metrics.scale,
        moved: false,
    });

    offset_monitor(&mut widget.monitors, "secondary", 0, 1200);

    let live_metrics = widget.layout_metrics(size);
    assert_eq!(live_metrics.origin_x, metrics.origin_x);
    assert_eq!(live_metrics.origin_y, metrics.origin_y);
    assert_eq!(live_metrics.scale, metrics.scale);
}

#[test]
fn canvas_origin_centers_far_away_arrangements() {
    let mut monitors = Settings::default().monitors;
    offset_monitor(&mut monitors, "primary", 10_000, -8_000);
    offset_monitor(&mut monitors, "secondary", 11_920, -8_000);
    let canvas_size = monitor_canvas_size(&monitors);

    let origin = monitor_canvas_origin(&monitors, canvas_size);

    assert_eq!(origin, (9616.0, -8756.0));
}

#[test]
fn normalize_monitors_keeps_layout_shape_centered_near_origin() {
    let mut monitors = Settings::default().monitors;
    offset_monitor(&mut monitors, "primary", 10_000, -8_000);
    offset_monitor(&mut monitors, "secondary", 11_920, -8_000);

    normalize_monitors_near_origin(&mut monitors);

    assert_eq!(
        monitor_arrangement_bounds(&monitors),
        Some((-1920, -540, 1920, 540))
    );
    assert!(monitor_borders_another(&monitors, "secondary", (0, -540)));
}

#[test]
fn live_drag_moves_only_selected_monitor_until_release() {
    let widget = MonitorLayoutWidget::new(Settings::default().monitors, None);
    let drag = MonitorDrag {
        id: "secondary".to_string(),
        start_pointer: Point::from((0.0, 0.0)),
        start_x: 1920,
        start_y: 0,
        origin_x: 0.0,
        origin_y: 0.0,
        scale: 1.0,
        moved: true,
    };

    let live = widget.monitors_for_drag(&drag, Point::from((0.0, 1200.0)), false);
    let released = widget.monitors_for_drag(&drag, Point::from((0.0, 1200.0)), true);

    let live_primary = live.iter().find(|monitor| monitor.id == "primary").unwrap();
    let live_secondary = live
        .iter()
        .find(|monitor| monitor.id == "secondary")
        .unwrap();
    assert_eq!((live_primary.x, live_primary.y), (0, 0));
    assert_eq!((live_secondary.x, live_secondary.y), (1919, 1080));

    assert_eq!(
        monitor_arrangement_bounds(&released),
        Some((-1919, -1080, 1920, 1080))
    );
}

#[test]
fn monitor_drag_snaps_gap_to_shared_border() {
    let monitors = Settings::default().monitors;

    let snapped = snap_monitor_to_neighbor(&monitors, "secondary", 2600, 120);

    assert_eq!(snapped, (1920, 120));
    assert!(monitor_borders_another(&monitors, "secondary", snapped));
}

#[test]
fn monitor_drag_snaps_to_border_and_preserves_overlap() {
    let monitors = Settings::default().monitors;

    let snapped = snap_monitor_to_neighbor(&monitors, "secondary", 600, -1500);

    assert_eq!(snapped, (600, -1080));
    assert!(monitor_borders_another(&monitors, "secondary", snapped));
}

#[test]
fn snap_position_rounds_to_nearest_grid_line() {
    assert_eq!(snap_position(14), 10);
    assert_eq!(snap_position(15), 20);
    assert_eq!(snap_position(-14), -10);
    assert_eq!(snap_position(-15), -20);
}

fn monitor_borders_another(monitors: &[MonitorConfig], id: &str, position: (i32, i32)) -> bool {
    let monitor = monitors.iter().find(|monitor| monitor.id == id).unwrap();
    monitors
        .iter()
        .any(|other| other.id != id && shares_border(position.0, position.1, monitor, other))
}

fn shares_border(x: i32, y: i32, monitor: &MonitorConfig, other: &MonitorConfig) -> bool {
    let width = monitor_logical_width(monitor);
    let height = monitor_logical_height(monitor);
    let other_width = monitor_logical_width(other);
    let other_height = monitor_logical_height(other);
    let horizontal_overlap = y < other.y + other_height && y + height > other.y;
    let vertical_overlap = x < other.x + other_width && x + width > other.x;
    let touches_left_or_right = x + width == other.x || x == other.x + other_width;
    let touches_top_or_bottom = y + height == other.y || y == other.y + other_height;

    (touches_left_or_right && horizontal_overlap) || (touches_top_or_bottom && vertical_overlap)
}
