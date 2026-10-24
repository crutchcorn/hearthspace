use super::*;
use hearthspace_ipc::{MonitorConfig, Settings};

fn geometry(name: &str, scale: i32) -> OutputGeometry {
    OutputGeometry {
        name: name.into(),
        size: (1920, 1080).into(),
        scale,
    }
}

fn preference(id: &str, x: i32, scale: i32) -> MonitorConfig {
    MonitorConfig {
        id: id.into(),
        name: id.into(),
        width: 1920,
        height: 1080,
        scale,
        x,
        y: 0,
    }
}

#[test]
fn explicit_primary_and_sorted_legacy_roles_survive_secondary_enumeration_changes() {
    let layout = OutputLayout::from_monitors(vec![
        preference("primary", 3840, 1),
        preference("secondary", 0, 1),
        preference("secondary-2", 1920, 1),
    ]);
    for secondary in [
        vec![geometry("eDP-1", 1), geometry("DP-1", 1)],
        vec![geometry("DP-1", 1), geometry("eDP-1", 1)],
    ] {
        let plan = output_locations_for_layout(geometry("DP-3", 1), secondary, &layout);
        assert_eq!(plan.primary, (3840, 0).into());
        assert_eq!(plan.secondary_location("DP-1"), (0, 0).into());
        assert_eq!(plan.secondary_location("eDP-1"), (1920, 0).into());
    }
}

#[test]
fn disconnected_history_and_retained_placeholders_do_not_change_physical_compatibility_layout() {
    let mut preferences = Settings::default().monitors;
    preferences.extend([
        preference("eDP-1", -1920, 2),
        preference("HDMI-A-1", 0, 1),
        preference("disconnected", -8000, 4),
    ]);
    let plan = output_locations_for_layout(
        geometry("HDMI-A-1", 1),
        vec![geometry("eDP-1", 1)],
        &OutputLayout::from_monitors(preferences),
    );
    assert_eq!(plan.primary, (960, 0).into());
    assert_eq!(plan.secondary_location("eDP-1"), (0, 0).into());
}

#[test]
fn newly_connected_monitor_keeps_backend_scale_after_first_connector_save() {
    let mut preferences = Settings::default().monitors;
    preferences.push(preference("eDP-1", 0, 2));
    let plan = output_locations_for_layout(
        geometry("eDP-1", 2),
        vec![geometry("DP-1", 2)],
        &OutputLayout::from_monitors(preferences),
    );
    assert_eq!(plan.secondary_scale("DP-1"), Some(2));
    assert_eq!(plan.secondary_location("DP-1"), (960, 0).into());
}

#[test]
fn resolved_connector_cannot_reuse_a_preference_through_its_display_name() {
    let mut saved = preference("DP-1", 0, 2);
    saved.name = "DP-2".into();
    let plan = output_locations_for_layout(
        geometry("DP-1", 1),
        vec![geometry("DP-2", 1)],
        &OutputLayout::from_monitors(vec![saved]),
    );
    assert_eq!(plan.primary_scale, 2);
    assert_eq!(plan.secondary_scale("DP-2"), Some(1));
    assert_eq!(plan.secondary_location("DP-2"), (960, 0).into());
}
