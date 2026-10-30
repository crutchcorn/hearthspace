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
    /// Current wl_output mode frequency, in millihertz; absent when not advertised.
    pub(crate) refresh_millihertz: Option<i32>,
}

/// Human-facing connector text; routing and saved preferences keep the full ID.
pub(crate) fn connector_label(id: &str) -> &str {
    hearthspace_common::output_identity::qualified_connector(id).unwrap_or(id)
}

pub(crate) fn built_in_display(description: &str, id: &str) -> bool {
    let connector = connector_label(id);
    connector.starts_with("eDP")
        || connector.starts_with("LVDS")
        || description.to_lowercase().contains("built-in")
}

pub(crate) fn display_name(description: &str, id: &str) -> String {
    if built_in_display(description, id) {
        return "Built-in display".into();
    }
    let connector = connector_label(id);
    // Smithay appends the exact output ID to its make/model description. Keep
    // product names while avoiding internal GPU paths and repeated connectors.
    let description = description
        .trim()
        .strip_suffix(&format!(" ({id})"))
        .unwrap_or(description.trim());
    let name = description
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != connector && *part != id && *part != "DRM")
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        "External display".into()
    } else {
        name
    }
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

/// Derive connected rows; the caller retains the complete saved preference list.
pub(crate) fn monitors_for_live_outputs(
    saved_monitors: &[MonitorConfig],
    live_outputs: &[LiveOutput],
    selected_primary: Option<&str>,
) -> Vec<MonitorConfig> {
    use hearthspace_common::output_identity::{
        SavedOutput, ordered_connectors, resolve_preferences,
    };
    let connectors: Vec<_> = live_outputs
        .iter()
        .map(|output| output.id.as_str())
        .collect();
    let defaults = Settings::default().monitors;
    let saved: Vec<_> = saved_monitors
        .iter()
        .map(|monitor| SavedOutput {
            id: &monitor.id,
            name: &monitor.name,
            placeholder: defaults.contains(monitor),
        })
        .collect();
    let preferences = resolve_preferences(&saved, &connectors, selected_primary);
    let mut rows = Vec::with_capacity(live_outputs.len());
    let mut unplaced = Vec::new();
    for connector in ordered_connectors(&connectors, selected_primary) {
        let index = live_outputs
            .iter()
            .position(|output| output.id == connector)
            .unwrap();
        let output = &live_outputs[index];
        let preference = preferences[index].map(|index| &saved_monitors[index]);
        let row = MonitorConfig {
            // Every new transaction uses connector identity, even when a legacy role supplied its preferences.
            id: output.id.clone(),
            name: output.name.clone(),
            width: output.width,
            height: output.height,
            scale: preference.map_or(output.scale, |saved| saved.scale).max(1),
            x: preference.map_or(0, |saved| saved.x),
            y: preference.map_or(0, |saved| saved.y),
        };
        // The selected primary falls back to the origin; only unconfigured
        // secondaries follow the rightmost configured/primary edge.
        if preference.is_none() && !rows.is_empty() {
            unplaced.push(rows.len());
        }
        rows.push(row);
    }
    let mut next_x = rows
        .iter()
        .enumerate()
        .filter(|(index, _)| !unplaced.contains(index))
        .map(|(_, monitor)| monitor.x.saturating_add(monitor_logical_width(monitor)))
        .max()
        .unwrap_or(0);
    for index in unplaced {
        rows[index].x = next_x;
        next_x = next_x.saturating_add(monitor_logical_width(&rows[index]));
    }
    if monitors_overlap(&rows) {
        place_monitors_edge_to_edge(&mut rows);
    }
    rows
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
    ((i64::from(size) + i64::from(scale) - 1) / i64::from(scale)).max(1) as i32
}

#[cfg(test)]
mod tests;
