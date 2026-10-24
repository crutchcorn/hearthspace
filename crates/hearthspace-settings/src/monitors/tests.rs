use super::*;

#[test]
fn retained_starter_preferences_do_not_override_new_outputs_after_a_connector_save() {
    let mut saved = Settings::default().monitors;
    let mut connector = saved[0].clone();
    connector.id = "eDP-1".into();
    connector.scale = 2;
    saved.push(connector);
    let outputs = ["eDP-1", "DP-1"].map(|id| LiveOutput {
        id: id.into(),
        name: id.into(),
        width: 1920,
        height: 1080,
        scale: 2,
        refresh_millihertz: None,
    });
    let rows = monitors_for_live_outputs(&saved, &outputs, Some("eDP-1"));
    assert_eq!(rows[1].scale, 2);
    assert_eq!(rows[1].x, 960);
    assert_eq!(saved.len(), 3);
}

fn offset_monitor(monitors: &mut [MonitorConfig], id: &str, x: i32, y: i32) {
    if let Some(monitor) = monitors.iter_mut().find(|monitor| monitor.id == id) {
        monitor.x = x;
        monitor.y = y;
    }
}

#[test]
fn live_outputs_hide_saved_disconnected_monitors() {
    let monitors = monitors_for_live_outputs(
        &Settings::default().monitors,
        &[LiveOutput {
            id: "eDP-1".to_string(),
            name: "Built-in Display".to_string(),
            width: 2256,
            height: 1504,
            scale: 1,
            refresh_millihertz: None,
        }],
        Some("eDP-1"),
    );

    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].id, "eDP-1");
    assert_eq!(monitors[0].name, "Built-in Display");
    assert_eq!((monitors[0].width, monitors[0].height), (2256, 1504));
}

#[test]
fn live_outputs_do_not_reuse_placeholder_positions_for_mixed_resolutions() {
    let monitors = monitors_for_live_outputs(
        &Settings::default().monitors,
        &[
            LiveOutput {
                id: "eDP-1".to_string(),
                name: "Built-in Display".to_string(),
                width: 2560,
                height: 1440,
                scale: 1,
                refresh_millihertz: None,
            },
            LiveOutput {
                id: "HDMI-A-1".to_string(),
                name: "External Monitor".to_string(),
                width: 1366,
                height: 768,
                scale: 1,
                refresh_millihertz: None,
            },
        ],
        Some("eDP-1"),
    );

    assert_eq!(monitors[0].id, "eDP-1");
    assert_eq!((monitors[0].x, monitors[0].y), (0, 0));
    assert_eq!(monitors[1].id, "HDMI-A-1");
    assert_eq!((monitors[1].x, monitors[1].y), (2560, 0));
}

#[test]
fn live_outputs_repair_overlapping_saved_active_monitors() {
    let saved = vec![
        MonitorConfig {
            id: "primary".to_string(),
            name: "Primary".to_string(),
            width: 1920,
            height: 1080,
            scale: 1,
            x: -1280,
            y: -736,
        },
        MonitorConfig {
            id: "secondary".to_string(),
            name: "Secondary".to_string(),
            width: 1920,
            height: 1080,
            scale: 1,
            x: -1920,
            y: -540,
        },
    ];

    let monitors = monitors_for_live_outputs(
        &saved,
        &[
            LiveOutput {
                id: "eDP-1".to_string(),
                name: "Built-in Display".to_string(),
                width: 2560,
                height: 1440,
                scale: 1,
                refresh_millihertz: None,
            },
            LiveOutput {
                id: "HDMI-A-1".to_string(),
                name: "External Monitor".to_string(),
                width: 1366,
                height: 768,
                scale: 1,
                refresh_millihertz: None,
            },
        ],
        Some("eDP-1"),
    );

    assert_eq!((monitors[0].x, monitors[0].y), (0, 0));
    assert_eq!((monitors[1].x, monitors[1].y), (2560, 0));
    assert!(!monitors_overlap(&monitors));
}

#[test]
fn live_outputs_preserve_saved_connector_positions() {
    let saved = vec![MonitorConfig {
        id: "HDMI-A-1".to_string(),
        name: "HDMI-A-1".to_string(),
        width: 1920,
        height: 1080,
        scale: 2,
        x: -1920,
        y: 200,
    }];

    let monitors = monitors_for_live_outputs(
        &saved,
        &[LiveOutput {
            id: "HDMI-A-1".to_string(),
            name: "External Monitor".to_string(),
            width: 2560,
            height: 1440,
            scale: 1,
            refresh_millihertz: None,
        }],
        Some("eDP-1"),
    );

    assert_eq!(monitors.len(), 1);
    assert_eq!(monitors[0].id, "HDMI-A-1");
    assert_eq!(monitors[0].name, "External Monitor");
    assert_eq!((monitors[0].x, monitors[0].y), (-1920, 200));
    assert_eq!((monitors[0].width, monitors[0].height), (2560, 1440));
    assert_eq!(monitors[0].scale, 2);
}

#[test]
fn live_outputs_use_reported_scale_for_new_monitors() {
    let monitors = monitors_for_live_outputs(
        &[],
        &[LiveOutput {
            id: "eDP-1".to_string(),
            name: "Built-in Display".to_string(),
            width: 2560,
            height: 1440,
            scale: 2,
            refresh_millihertz: None,
        }],
        Some("eDP-1"),
    );

    assert_eq!(monitors[0].scale, 2);
    assert_eq!(monitor_logical_width(&monitors[0]), 1280);
}

#[test]
fn scaling_monitor_moves_neighbors_to_scaled_logical_edge() {
    let mut monitors = Settings::default().monitors;

    recalculate_monitor_positions_for_scale(&mut monitors, "primary", 2);

    let primary = monitors
        .iter()
        .find(|monitor| monitor.id == "primary")
        .unwrap();
    let secondary = monitors
        .iter()
        .find(|monitor| monitor.id == "secondary")
        .unwrap();
    assert_eq!(primary.scale, 2);
    assert_eq!(monitor_logical_width(primary), 960);
    assert_eq!((secondary.x, secondary.y), (960, 0));
    assert!(!monitors_overlap(&monitors));
}

#[test]
fn scaling_monitor_moves_outputs_below_to_scaled_logical_edge() {
    let mut monitors = Settings::default().monitors;
    offset_monitor(&mut monitors, "secondary", 0, 1080);

    recalculate_monitor_positions_for_scale(&mut monitors, "primary", 2);

    let secondary = monitors
        .iter()
        .find(|monitor| monitor.id == "secondary")
        .unwrap();
    assert_eq!((secondary.x, secondary.y), (0, 540));
    assert!(!monitors_overlap(&monitors));
}
