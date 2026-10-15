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
    let path = dir.path().join("favorites.json");
    state.start("Alpha.desktop");
    let generation = state.recording.as_ref().unwrap().generation;
    state.stroke("Alpha.desktop", generation, shape());
    assert!(state.recording.as_ref().unwrap().valid);
    assert!(!path.exists());
    assert!(state.entries().is_empty());
    state.save();
    assert!(state.recording.is_none());
    let saved = FavoriteStore::load(path).unwrap();
    assert_eq!(saved.entries().len(), 1);
    assert_eq!(saved.entries()[0].app_id, "Alpha.desktop");
    assert!(saved.entries()[0].gesture.is_some());
}

#[test]
fn cancel_and_retry_reject_late_strokes_without_creating_an_empty_entry() {
    let (dir, mut state) = fixture();
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
    assert!(state.entries().is_empty());
    assert!(!dir.path().join("favorites.json").exists());
}

#[test]
fn similar_and_invalid_shapes_never_add_an_empty_entry_or_replace_an_existing_shape() {
    let (dir, mut state) = fixture();
    state
        .store
        .as_mut()
        .unwrap()
        .set_gesture("Alpha.desktop", &shape())
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
    assert_eq!(state.entries().len(), 1);
    assert_eq!(fs::read(dir.path().join("favorites.json")).unwrap(), before);
}

#[test]
fn missing_apps_keep_their_assignment_until_removed_but_cannot_start_a_new_recording() {
    let (dir, mut state) = fixture();
    state
        .store
        .as_mut()
        .unwrap()
        .set_gesture("Alpha.desktop", &shape())
        .unwrap();
    fs::remove_file(dir.path().join("applications/Alpha.desktop")).unwrap();
    state.catalog = AppCatalog::load_from_data_dirs(vec![dir.path().to_owned()]);
    state.start("Alpha.desktop");
    assert!(state.recording.is_none());
    state.start("unknown.desktop");
    assert_eq!(state.assigned().count(), 1);
    state.remove_gesture("Alpha.desktop");
    assert!(state.entries().is_empty());
    assert!(
        FavoriteStore::load(dir.path().join("favorites.json"))
            .unwrap()
            .entries()
            .is_empty()
    );
}

#[test]
fn malformed_configuration_never_falls_back_to_a_writable_empty_store() {
    let (dir, state) = fixture();
    let path = dir.path().join("favorites.json");
    fs::write(&path, b"damaged config").unwrap();
    let mut state = FavoriteSettings::from_source(FavoriteStore::load(&path), state.catalog);
    assert!(state.store.is_none());
    state.start("Alpha.desktop");
    assert!(state.recording.is_none());
    state.save();
    assert_eq!(fs::read(&path).unwrap(), b"damaged config");
}

#[test]
fn another_editors_change_is_preserved_and_the_unsaved_candidate_remains_available() {
    let (dir, mut state) = fixture();
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
    assert!(state.entries().is_empty());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn removing_a_gesture_deletes_the_assignment_rather_than_leaving_an_empty_entry() {
    let (dir, mut state) = fixture();
    state
        .store
        .as_mut()
        .unwrap()
        .set_gesture("Alpha.desktop", &shape())
        .unwrap();
    state.remove_gesture("Alpha.desktop");
    assert!(state.entries().is_empty());
    assert!(
        FavoriteStore::load(dir.path().join("favorites.json"))
            .unwrap()
            .entries()
            .is_empty()
    );
}

#[test]
fn legacy_empty_entries_are_hidden_and_cancelling_does_not_rewrite_them() {
    let (dir, mut state) = fixture();
    state.store.as_mut().unwrap().add("Alpha.desktop").unwrap();
    let path = dir.path().join("favorites.json");
    let before = fs::read(&path).unwrap();
    assert_eq!(state.assigned().count(), 0);
    state.start("Alpha.desktop");
    state.cancel();
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(state.assigned().count(), 0);
}

#[test]
fn cancelling_a_changed_gesture_preserves_the_existing_assignment() {
    let (dir, mut state) = fixture();
    state
        .store
        .as_mut()
        .unwrap()
        .set_gesture("Alpha.desktop", &shape())
        .unwrap();
    let path = dir.path().join("favorites.json");
    let before = fs::read(&path).unwrap();
    state.start("Alpha.desktop");
    let generation = state.recording.as_ref().unwrap().generation;
    state.stroke(
        "Alpha.desktop",
        generation,
        vec![[40.0, 40.0], [40.0, 140.0], [160.0, 140.0]],
    );
    state.cancel();
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(state.assigned().count(), 1);
}
