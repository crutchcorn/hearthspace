use smithay::output::{PhysicalProperties, Subpixel};
use wayland_server::Display;

use super::*;
use crate::compositor::output::OutputLayout;

fn descriptor(name: &str) -> OutputDescriptor {
    OutputDescriptor {
        name: name.into(),
        properties: PhysicalProperties {
            size: (300, 200).into(),
            subpixel: Subpixel::Unknown,
            make: "Test".into(),
            model: name.into(),
            serial_number: name.into(),
        },
        size: (1280, 800).into(),
        scale: 1,
        refresh: 60_000,
    }
}

fn outputs(display: &Display<App>) -> OutputSet {
    OutputSet::new(
        create_output_record(&display.handle(), descriptor("eDP-1"), (0, 0).into()),
        OutputLayout::default(),
    )
}

#[test]
fn promoted_secondary_keeps_its_connector_identity_and_wayland_global() {
    let display = Display::<App>::new().unwrap();
    let mut outputs = outputs(&display);
    outputs.sync_connectors(
        &display.handle(),
        vec![descriptor("eDP-1"), descriptor("DP-1")],
    );
    let internal = outputs.primary.output.clone();
    let external = outputs.secondary[0].output.clone();
    let external_global = outputs.secondary[0].global_id.clone();
    assert!(outputs.sync_connectors(&display.handle(), vec![descriptor("DP-1")]));
    assert_eq!(outputs.primary.name, "DP-1");
    assert_eq!(outputs.primary.output, external);
    assert_eq!(outputs.primary.global_id, external_global);
    assert_ne!(outputs.primary.output, internal);
    assert!(outputs.output_for_name(Some("eDP-1")).is_none());
    assert_eq!(outputs.primary.output.name(), outputs.primary.name);
    assert_eq!(outputs.output_debug().len(), 1);
}

#[test]
fn changing_primary_preserves_both_connected_outputs_without_duplicates() {
    let display = Display::<App>::new().unwrap();
    let mut outputs = outputs(&display);
    let internal = outputs.primary.output.clone();
    assert!(!outputs.sync_connectors(
        &display.handle(),
        vec![descriptor("DP-1"), descriptor("eDP-1")],
    ));
    assert_eq!(outputs.primary.name, "DP-1");
    assert_eq!(outputs.secondary.len(), 1);
    assert_eq!(outputs.secondary[0].output, internal);
    assert_eq!(outputs.secondary[0].name, "eDP-1");
    assert!(!outputs.sync_connectors(
        &display.handle(),
        vec![descriptor("eDP-1"), descriptor("DP-1")],
    ));
    assert_eq!(outputs.primary.output, internal);
    assert_eq!(outputs.secondary.len(), 1);
    assert_eq!(outputs.secondary[0].name, "DP-1");
}

#[test]
fn no_outputs_exposes_no_stale_target_and_replug_creates_a_fresh_global() {
    let display = Display::<App>::new().unwrap();
    let mut outputs = outputs(&display);
    let old_global = outputs.primary.global_id.clone();
    let old_output = outputs.primary.output.clone();
    assert!(outputs.sync_connectors(&display.handle(), Vec::new()));
    assert!(!outputs.sync_connectors(&display.handle(), Vec::new()));
    assert!(outputs.output_debug().is_empty());
    assert!(outputs.logical_rects().is_empty());
    assert!(outputs.output_for_name(None).is_none());
    assert!(outputs.output_for_name(Some("eDP-1")).is_none());
    // An already disabled global cannot be revived; announce a fresh object
    // so shell and lock clients receive a real registry add after removal.
    assert!(!outputs.sync_connectors(&display.handle(), vec![descriptor("eDP-1")]));
    assert!(outputs.primary.advertised);
    assert_ne!(outputs.primary.global_id, old_global);
    assert_ne!(outputs.primary.output, old_output);
    assert_eq!(outputs.primary.name, outputs.primary.output.name());
    assert_eq!(outputs.output_debug().len(), 1);
}
