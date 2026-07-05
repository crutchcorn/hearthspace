use hearthspace_ipc::{MonitorConfig, Settings};

pub(crate) const MONITOR_SCALE_OPTIONS: [i32; 4] = [1, 2, 3, 4];
pub(crate) const EMPTY_MONITOR_CANVAS_WIDTH: i32 = 1920;
pub(crate) const EMPTY_MONITOR_CANVAS_HEIGHT: i32 = 1080;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveOutput {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) scale: i32,
}

pub(crate) fn recalculate_monitor_positions_for_scale(
    monitors: &mut [MonitorConfig],
    id: &str,
    scale: i32,
) {
    let Some(index) = monitors.iter().position(|monitor| monitor.id == id) else {
        return;
    };
    let old_monitor = monitors[index].clone();
    let old_width = monitor_logical_width(&old_monitor);
    let old_height = monitor_logical_height(&old_monitor);
    let old_right = old_monitor.x + old_width;
    let old_bottom = old_monitor.y + old_height;

    monitors[index].scale = scale.max(1);
    let new_width = monitor_logical_width(&monitors[index]);
    let new_height = monitor_logical_height(&monitors[index]);
    let dx = new_width - old_width;
    let dy = new_height - old_height;

    for (monitor_index, monitor) in monitors.iter_mut().enumerate() {
        if monitor_index == index {
            continue;
        }
        if monitor.x >= old_right {
            monitor.x += dx;
        }
        if monitor.y >= old_bottom {
            monitor.y += dy;
        }
    }

    if monitors_overlap(monitors) {
        place_monitors_edge_to_edge(monitors);
    }
}

pub(crate) fn settings_with_live_outputs(
    mut settings: Settings,
    live_outputs: &[LiveOutput],
) -> Settings {
    if live_outputs.is_empty() {
        return settings;
    }

    settings.monitors = monitors_for_live_outputs(&settings.monitors, live_outputs);
    settings
}

fn monitors_for_live_outputs(
    saved_monitors: &[MonitorConfig],
    live_outputs: &[LiveOutput],
) -> Vec<MonitorConfig> {
    let mut monitors = Vec::with_capacity(live_outputs.len());
    let mut next_x = 0;
    let saved_monitors_are_placeholders = default_placeholder_monitors(saved_monitors);
    for (index, output) in live_outputs.iter().enumerate() {
        let role_id = monitor_role_id(index);
        let saved = (!saved_monitors_are_placeholders)
            .then(|| {
                saved_monitors
                    .iter()
                    .find(|monitor| monitor.id == output.id || monitor.name == output.name)
                    .or_else(|| saved_monitors.iter().find(|monitor| monitor.id == role_id))
            })
            .flatten();
        let (id, scale, x, y) = if let Some(saved) = saved {
            (saved.id.clone(), saved.scale.max(1), saved.x, saved.y)
        } else if saved_monitors_are_placeholders && index < 2 {
            (role_id, output.scale.max(1), next_x, 0)
        } else {
            (output.id.clone(), output.scale.max(1), next_x, 0)
        };
        monitors.push(MonitorConfig {
            id,
            name: output.name.clone(),
            width: output.width,
            height: output.height,
            scale,
            x,
            y,
        });
        next_x += logical_dimension(output.width, scale);
    }
    if monitors_overlap(&monitors) {
        place_monitors_edge_to_edge(&mut monitors);
    }
    monitors
}

fn place_monitors_edge_to_edge(monitors: &mut [MonitorConfig]) {
    let mut next_x = 0;
    for monitor in monitors {
        monitor.x = next_x;
        monitor.y = 0;
        next_x += monitor_logical_width(monitor);
    }
}

pub(crate) fn monitors_overlap(monitors: &[MonitorConfig]) -> bool {
    monitors.iter().enumerate().any(|(index, monitor)| {
        monitors
            .iter()
            .skip(index + 1)
            .any(|other| monitor_rects_overlap(monitor, other))
    })
}

fn monitor_rects_overlap(a: &MonitorConfig, b: &MonitorConfig) -> bool {
    a.x < b.x + monitor_logical_width(b)
        && a.x + monitor_logical_width(a) > b.x
        && a.y < b.y + monitor_logical_height(b)
        && a.y + monitor_logical_height(a) > b.y
}

pub(crate) fn monitor_logical_width(monitor: &MonitorConfig) -> i32 {
    logical_dimension(monitor.width, monitor.scale)
}

pub(crate) fn monitor_logical_height(monitor: &MonitorConfig) -> i32 {
    logical_dimension(monitor.height, monitor.scale)
}

fn logical_dimension(size: i32, scale: i32) -> i32 {
    let size = size.max(1);
    let scale = scale.max(1);
    ((size + scale - 1) / scale).max(1)
}

fn default_placeholder_monitors(monitors: &[MonitorConfig]) -> bool {
    monitors == Settings::default().monitors
}

fn monitor_role_id(index: usize) -> String {
    if index == 0 {
        "primary".to_string()
    } else if index == 1 {
        "secondary".to_string()
    } else {
        format!("secondary-{index}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            }],
        );

        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].id, "primary");
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
                },
                LiveOutput {
                    id: "HDMI-A-1".to_string(),
                    name: "External Monitor".to_string(),
                    width: 1366,
                    height: 768,
                    scale: 1,
                },
            ],
        );

        assert_eq!(monitors[0].id, "primary");
        assert_eq!((monitors[0].x, monitors[0].y), (0, 0));
        assert_eq!(monitors[1].id, "secondary");
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
                },
                LiveOutput {
                    id: "HDMI-A-1".to_string(),
                    name: "External Monitor".to_string(),
                    width: 1366,
                    height: 768,
                    scale: 1,
                },
            ],
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
            }],
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
            }],
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
}
