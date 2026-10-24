use super::*;
use hearthspace_common::frame_layout::{FrameEdge, ObjectPlacement};

#[test]
fn durable_group_members_survive_releasing_all_claims_and_reloading() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut store = LayoutStore::load(Some(path.clone()));
    store.claim(73, "org.example.App".into());
    let canonical = store.canonical_target(&LayoutTarget::new(TargetKind::Window, "73"));
    let mut layout = FrameLayout::default();
    let apps = LayoutTarget::widget("apps");
    layout.set_placement(&apps, ObjectPlacement::frame(FrameEdge::Right, 0.5));
    let group = layout.merge(&canonical, &apps, false).unwrap();
    store.layout.frames.insert("output".into(), layout.clone());
    store.claimed.clear();
    store.save().unwrap();
    store.save().unwrap();

    let mut restarted = LayoutStore::load(Some(path));
    assert_eq!(restarted.frame_layouts()["output"], layout);
    assert_eq!(restarted.runtime_id(&canonical), None);
    restarted.claim(902, "org.example.App".into());
    assert_eq!(restarted.runtime_id(&canonical), Some(902));
    assert_eq!(
        restarted.canonical_target(&LayoutTarget::new(TargetKind::Window, "902")),
        canonical
    );
    assert_eq!(
        restarted.frame_layouts()["output"]
            .group_for(&canonical)
            .unwrap()
            .id,
        group.id
    );
    assert_eq!(restarted.frame_layouts()["output"], layout);
}

#[test]
fn app_slots_reuse_only_released_bindings_and_leave_unclaimed_windows_transient() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = LayoutStore::load(Some(directory.path().join("layout.json")));
    store.claim(10, "editor".into());
    store.claim(11, "editor".into());
    let first = store.canonical_target(&LayoutTarget::new(TargetKind::Window, "10"));
    let second = store.canonical_target(&LayoutTarget::new(TargetKind::Window, "11"));
    assert_ne!(first, second);
    store.claimed.remove(&10);
    store.claim(12, "other-app".into());
    assert_eq!(store.runtime_id(&first), None);
    store.claim(13, "editor".into());
    assert_eq!(store.runtime_id(&first), Some(13));
    assert_eq!(store.runtime_id(&second), Some(11));
    assert_eq!(
        store.canonical_target(&LayoutTarget::new(TargetKind::Window, "99")),
        LayoutTarget::new(TargetKind::Window, "99")
    );
}

#[test]
fn legacy_runtime_references_cannot_attach_to_reused_ids_after_reload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut store = LayoutStore::load(Some(path.clone()));
    let window = LayoutTarget::new(TargetKind::Window, "1");
    let apps = LayoutTarget::widget("apps");
    let durable = LayoutTarget::new(TargetKind::Window, saved_id("editor", 0));
    let mut layout = FrameLayout::default();
    layout.set_placement(&apps, ObjectPlacement::frame(FrameEdge::Right, 0.5));
    layout.merge(&window, &apps, false).unwrap();
    layout.set_placement(&durable, ObjectPlacement::frame(FrameEdge::Top, 0.5));
    store.layout.frames.insert("output".into(), layout);
    store.save().unwrap();

    let mut restarted = LayoutStore::load(Some(path));
    restarted.claim(1, "unrelated".into());
    let restored = &restarted.frame_layouts()["output"];
    assert!(runtime_targets(restored).is_empty());
    assert!(restored.placement(&apps).is_frame());
    assert!(restored.placement(&durable).is_frame());
    restored.validate().unwrap();
}
