use hearthspace_common::widgets::{InstanceId, Preferences, WidgetKind, WidgetStore};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct DesktopState {
    widgets: WidgetStore,
    placement: Vec<(String, f64, f64)>,
}

#[test]
fn nested_persistence_preserves_identity_and_does_not_recycle_removed_copies() {
    let mut widgets = WidgetStore::new();
    let kept = widgets
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    let retired = widgets
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    widgets.remove(retired).unwrap();
    let desktop = DesktopState {
        widgets,
        placement: vec![(kept.to_string(), -120.0, 450.0)],
    };
    let encoded = serde_json::to_string(&desktop).unwrap();
    let mut restored: DesktopState = serde_json::from_str(&encoded).unwrap();
    let replacement = restored
        .widgets
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    assert_eq!(restored.placement, desktop.placement);
    assert!(restored.widgets.get(kept).is_some());
    assert!(restored.widgets.get(retired).is_none());
    assert_ne!(replacement, retired);
    assert_eq!(kept.to_string().parse::<InstanceId>().unwrap(), kept);
    let noncanonical = kept.to_string().replace(":1", ":01");
    assert!(noncanonical.parse::<InstanceId>().is_err());
}

#[test]
fn nested_persistence_rejects_a_counter_that_could_reuse_an_identity() {
    let mut widgets = WidgetStore::new();
    widgets
        .add(WidgetKind::Power, Preferences::default())
        .unwrap();
    let mut value = serde_json::to_value(&widgets).unwrap();
    value["next_serial"] = serde_json::json!(1);
    assert!(serde_json::from_value::<WidgetStore>(value).is_err());
}
