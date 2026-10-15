use super::*;
use std::fs;

pub(crate) fn fixture() -> (tempfile::TempDir, FavoriteSettings) {
    let directory = tempfile::tempdir().unwrap();
    let apps = directory.path().join("applications");
    fs::create_dir(&apps).unwrap();
    for name in ["Alpha", "Beta"] {
        fs::write(
            apps.join(format!("{name}.desktop")),
            format!("[Desktop Entry]\nType=Application\nName={name}\nExec=false\n"),
        )
        .unwrap();
    }
    let catalog = AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()]);
    let favorites = FavoriteSettings::from_source(
        FavoriteStore::load(directory.path().join("favorites.json")),
        catalog,
    );
    (directory, favorites)
}

pub(crate) fn shape() -> Vec<Point> {
    vec![[30.0, 40.0], [90.0, 130.0], [150.0, 40.0]]
}

#[test]
fn recording_is_a_draft_until_explicit_save() {
    let (dir, mut state) = fixture();
    state.add("Alpha.desktop");
    let path = dir.path().join("favorites.json");
    let before = fs::read(&path).unwrap();
    state.start("Alpha.desktop");
    let generation = state.recording.as_ref().unwrap().generation;
    state.stroke("Alpha.desktop", generation, shape());
    assert!(state.recording.as_ref().unwrap().valid);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(state.entries()[0].gesture.is_none());
    state.save();
    assert!(state.recording.is_none());
    assert!(
        FavoriteStore::load(path).unwrap().entries()[0]
            .gesture
            .is_some()
    );
}

#[test]
fn cancel_and_retry_reject_late_strokes_and_leave_saved_shape_untouched() {
    let (dir, mut state) = fixture();
    state.add("Alpha.desktop");
    state.start("Alpha.desktop");
    let old = state.recording.as_ref().unwrap().generation;
    state.retry();
    let new = state.recording.as_ref().unwrap().generation;
    assert_ne!(old, new);
    state.stroke("Alpha.desktop", old, shape());
    assert!(!state.recording.as_ref().unwrap().valid);
    state.stroke("Beta.desktop", new, shape());
    assert!(!state.recording.as_ref().unwrap().valid);
    state.cancel();
    state.stroke("Alpha.desktop", new, shape());
    state.save();
    assert!(state.recording.is_none());
    assert!(
        FavoriteStore::load(dir.path().join("favorites.json"))
            .unwrap()
            .entries()[0]
            .gesture
            .is_none()
    );
}

#[test]
fn similar_and_invalid_shapes_are_rejected_without_losing_the_previous_assignment() {
    let (dir, mut state) = fixture();
    state.add("Alpha.desktop");
    state.add("Beta.desktop");
    state
        .store
        .as_mut()
        .unwrap()
        .assign("Alpha.desktop", &shape())
        .unwrap();
    let before = fs::read(dir.path().join("favorites.json")).unwrap();
    state.start("Beta.desktop");
    let generation = state.recording.as_ref().unwrap().generation;
    state.stroke("Beta.desktop", generation, vec![[0.0, 0.0], [1.0, 1.0]]);
    assert!(!state.recording.as_ref().unwrap().valid);
    state.save();
    state.stroke("Beta.desktop", generation, shape());
    state.save();
    assert!(state.message.as_ref().unwrap().contains("similar"));
    assert!(state.recording.is_some());
    assert_eq!(fs::read(dir.path().join("favorites.json")).unwrap(), before);
}

#[test]
fn missing_apps_can_be_removed_but_cannot_start_an_assignment() {
    let (dir, mut state) = fixture();
    state.add("Alpha.desktop");
    fs::remove_file(dir.path().join("applications/Alpha.desktop")).unwrap();
    state.catalog = AppCatalog::load_from_data_dirs(vec![dir.path().to_owned()]);
    state.start("Alpha.desktop");
    assert!(state.recording.is_none());
    state.add("unknown.desktop");
    assert_eq!(state.entries().len(), 1);
    state.remove("Alpha.desktop");
    assert!(state.entries().is_empty());
}

#[test]
fn malformed_configuration_never_falls_back_to_a_writable_empty_store() {
    let (dir, state) = fixture();
    let path = dir.path().join("favorites.json");
    fs::write(&path, b"damaged config").unwrap();
    let mut state = FavoriteSettings::from_source(FavoriteStore::load(&path), state.catalog);
    assert!(state.store.is_none());
    state.add("Alpha.desktop");
    state.start("Alpha.desktop");
    state.save();
    assert_eq!(fs::read(&path).unwrap(), b"damaged config");
}

#[test]
fn another_editors_change_is_preserved_and_the_unsaved_candidate_remains_available() {
    let (dir, mut state) = fixture();
    state.add("Alpha.desktop");
    state.start("Alpha.desktop");
    let generation = state.recording.as_ref().unwrap().generation;
    state.stroke("Alpha.desktop", generation, shape());
    let path = dir.path().join("favorites.json");
    let mut other = FavoriteStore::load(&path).unwrap();
    other.add("Beta.desktop").unwrap();
    let before = fs::read(&path).unwrap();
    state.save();
    assert!(state.recording.as_ref().unwrap().valid);
    assert!(state.message.as_ref().unwrap().contains("changed"));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn remove_gesture_keeps_favorite_and_its_stable_identifier() {
    let (_, mut state) = fixture();
    // Keep this test in memory so the fixture directory can be discarded.
    state.store = Some(FavoriteStore::in_memory());
    state.add("Alpha.desktop");
    state
        .store
        .as_mut()
        .unwrap()
        .assign("Alpha.desktop", &shape())
        .unwrap();
    state.clear("Alpha.desktop");
    assert_eq!(state.entries()[0].app_id, "Alpha.desktop");
    assert!(state.entries()[0].gesture.is_none());
}
