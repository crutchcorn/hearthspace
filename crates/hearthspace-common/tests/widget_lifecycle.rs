use hearthspace_common::widgets::{
    Error, MAX_DATE_FORMAT_BYTES, MAX_INSTANCES, MAX_SNAPSHOT_BYTES, Preferences, WidgetKind,
    WidgetStore,
};
use serde_json::{Value, json};

fn clock_preferences(format: &str) -> Preferences {
    Preferences {
        hide_when_unavailable: false,
        expanded_details: true,
        clock_date_format: Some(format.to_owned()),
    }
}

#[test]
fn startup_and_empty_restart_never_synthesize_widgets() {
    let store = WidgetStore::new();
    assert_eq!(store.instances().count(), 0);
    let snapshot = store.to_json().unwrap();
    let restored = WidgetStore::from_json(&snapshot).unwrap();
    assert_eq!(restored.instances().count(), 0);
}

#[test]
fn duplicate_clocks_have_independent_identities_and_preferences() {
    let mut store = WidgetStore::new();
    let first = store
        .add(WidgetKind::Clock, clock_preferences("%F"))
        .unwrap();
    let second = store
        .add(WidgetKind::Clock, clock_preferences("%a"))
        .unwrap();
    assert_ne!(first, second);
    store
        .set_preferences(first, clock_preferences("%B %e"))
        .unwrap();
    assert_eq!(
        store.get(first).unwrap().preferences(),
        &clock_preferences("%B %e")
    );
    assert_eq!(
        store.get(second).unwrap().preferences(),
        &clock_preferences("%a")
    );
    store.remove(first).unwrap();
    assert_eq!(store.get(second).unwrap().kind(), WidgetKind::Clock);
    assert_eq!(store.instances().count(), 1);
}

#[test]
fn retired_identity_stays_retired_across_empty_restart_and_readd() {
    let mut store = WidgetStore::new();
    let retired = store
        .add(WidgetKind::Sound, Preferences::default())
        .unwrap();
    store.remove(retired).unwrap();
    let snapshot = store.to_json().unwrap();
    let mut restored = WidgetStore::from_json(&snapshot).unwrap();
    let replacement = restored
        .add(WidgetKind::Sound, Preferences::default())
        .unwrap();
    assert_ne!(retired, replacement);
    let before = restored.to_json().unwrap();
    assert_eq!(
        restored.remove(retired),
        Err(Error::UnknownInstance(retired))
    );
    assert_eq!(
        restored.set_preferences(retired, Preferences::default()),
        Err(Error::UnknownInstance(retired))
    );
    assert_eq!(restored.to_json().unwrap(), before);
    assert!(restored.get(replacement).is_some());
}

#[test]
fn independently_created_stores_cannot_accept_each_others_ids() {
    let mut first = WidgetStore::new();
    let mut second = WidgetStore::new();
    let foreign = first
        .add(WidgetKind::Battery, Preferences::default())
        .unwrap();
    let local = second
        .add(WidgetKind::Battery, Preferences::default())
        .unwrap();
    assert_ne!(foreign, local);
    assert_eq!(second.remove(foreign), Err(Error::UnknownInstance(foreign)));
    assert!(second.get(local).is_some());
}

#[test]
fn every_catalog_kind_roundtrips_without_duplicates_or_resurrection() {
    let mut store = WidgetStore::new();
    let kinds = [
        WidgetKind::Notifications,
        WidgetKind::Clock,
        WidgetKind::Wifi,
        WidgetKind::Sound,
        WidgetKind::Brightness,
        WidgetKind::Battery,
        WidgetKind::ApplicationTray,
        WidgetKind::Power,
        WidgetKind::CanvasTools,
        WidgetKind::AppDock,
        WidgetKind::AppLauncher,
    ];
    let mut ids = Vec::new();
    for kind in kinds {
        ids.push(store.add(kind, Preferences::default()).unwrap());
    }
    store.remove(ids[0]).unwrap();
    let duplicate = store
        .add(WidgetKind::Clock, clock_preferences("%F"))
        .unwrap();
    let snapshot = store.to_json().unwrap();
    let mut restored = WidgetStore::from_json(&snapshot).unwrap();
    assert_eq!(restored.to_json().unwrap(), snapshot);
    assert_eq!(restored.instances().count(), kinds.len());
    assert!(restored.get(ids[0]).is_none());
    assert_eq!(
        restored.get(duplicate).unwrap().preferences(),
        &clock_preferences("%F")
    );
    let fresh = restored
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    assert!(!ids.contains(&fresh));
    assert_ne!(duplicate, fresh);
}

#[test]
fn invalid_preferences_reject_add_and_update_without_changing_any_state() {
    let mut store = WidgetStore::new();
    let id = store
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    let before = store.to_json().unwrap();
    let oversized = "x".repeat(MAX_DATE_FORMAT_BYTES + 1);
    assert!(
        store
            .add(WidgetKind::Clock, clock_preferences(&oversized))
            .is_err()
    );
    assert!(store.set_preferences(id, clock_preferences("")).is_err());
    assert!(
        store
            .set_preferences(id, clock_preferences("%F\n"))
            .is_err()
    );
    assert!(
        store
            .add(WidgetKind::Battery, clock_preferences("%F"))
            .is_err()
    );
    assert_eq!(store.to_json().unwrap(), before);
}

#[test]
fn capacity_failure_is_atomic_and_a_retired_slot_can_be_reused_with_a_fresh_id() {
    let mut store = WidgetStore::new();
    let first = store
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    for _ in 1..MAX_INSTANCES {
        store
            .add(WidgetKind::Clock, Preferences::default())
            .unwrap();
    }
    let before = store.to_json().unwrap();
    assert_eq!(
        store.add(WidgetKind::Clock, Preferences::default()),
        Err(Error::TooManyInstances)
    );
    assert_eq!(store.to_json().unwrap(), before);
    store.remove(first).unwrap();
    let replacement = store
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    assert_ne!(first, replacement);
    assert_eq!(store.instances().count(), MAX_INSTANCES);
}

fn snapshot_with_clock() -> Value {
    let mut store = WidgetStore::new();
    store
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    let snapshot = store.to_json().unwrap();
    serde_json::from_str(&snapshot).unwrap()
}

#[test]
fn duplicated_persisted_ids_are_rejected_instead_of_silently_overwritten() {
    let mut snapshot = snapshot_with_clock();
    let duplicate = snapshot["instances"][0].clone();
    snapshot["instances"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(matches!(
        WidgetStore::from_json(&snapshot.to_string()),
        Err(Error::InvalidSnapshot("duplicate instance identity"))
    ));
}

#[test]
fn persisted_high_water_mark_must_exceed_all_live_ids() {
    let mut snapshot = snapshot_with_clock();
    snapshot["next_serial"] = json!(1);
    assert!(WidgetStore::from_json(&snapshot.to_string()).is_err());
}

#[test]
fn a_foreign_persisted_id_is_rejected() {
    let mut snapshot = snapshot_with_clock();
    let foreign = snapshot_with_clock();
    snapshot["instances"][0]["id"] = foreign["instances"][0]["id"].clone();
    assert!(WidgetStore::from_json(&snapshot.to_string()).is_err());
}

#[test]
fn unknown_schema_fields_and_unbounded_snapshots_are_rejected() {
    let mut snapshot = snapshot_with_clock();
    snapshot["schema_version"] = json!(2);
    assert!(matches!(
        WidgetStore::from_json(&snapshot.to_string()),
        Err(Error::UnsupportedSchema(2))
    ));
    snapshot["schema_version"] = json!(1);
    snapshot["surprise"] = json!(true);
    assert!(WidgetStore::from_json(&snapshot.to_string()).is_err());
    let oversized = " ".repeat(MAX_SNAPSHOT_BYTES + 1);
    assert!(matches!(
        WidgetStore::from_json(&oversized),
        Err(Error::InputTooLarge)
    ));
}

#[test]
fn invalid_persisted_preferences_fail_before_returning_a_store() {
    let mut snapshot = snapshot_with_clock();
    snapshot["instances"][0]["preferences"]["clock_date_format"] = json!("");
    assert!(WidgetStore::from_json(&snapshot.to_string()).is_err());
    snapshot["instances"][0]["preferences"]["clock_date_format"] = json!("%F");
    snapshot["instances"][0]["kind"] = json!("battery");
    assert!(WidgetStore::from_json(&snapshot.to_string()).is_err());
}

#[test]
fn exhausted_identity_sequence_rejects_add_without_wraparound() {
    let mut snapshot = snapshot_with_clock();
    snapshot["next_serial"] = json!(u64::MAX);
    let mut store = WidgetStore::from_json(&snapshot.to_string()).unwrap();
    let before = store.to_json().unwrap();
    assert_eq!(
        store.add(WidgetKind::Clock, Preferences::default()),
        Err(Error::IdentityExhausted)
    );
    assert_eq!(store.to_json().unwrap(), before);
}
