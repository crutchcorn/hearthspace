use super::*;

const ELBOW: &[Point] = &[[0.0, 0.0], [0.0, 150.0], [150.0, 150.0]];
const LINE: &[Point] = &[[0.0, 0.0], [150.0, 0.0]];

#[test]
fn gesture_upsert_needs_no_prior_favorite_and_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gestures.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    store.set_gesture("editor.desktop", ELBOW).unwrap();
    store.set_gesture("editor.desktop", LINE).unwrap();
    let saved = FavoriteStore::load(path).unwrap();
    assert_eq!(saved.entries().len(), 1);
    assert_eq!(
        saved.recognize(LINE).unwrap(),
        Recognition::Match("editor.desktop".into())
    );
    assert_eq!(saved.recognize(ELBOW).unwrap(), Recognition::NoMatch);
}

#[test]
fn invalid_or_conflicting_gesture_never_creates_an_empty_assignment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gestures.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    assert!(store.set_gesture("editor.desktop", &[[0.0, 0.0]]).is_err());
    assert!(store.set_gesture("bad\napp", ELBOW).is_err());
    assert!(!path.exists());
    store.set_gesture("editor.desktop", ELBOW).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(store.set_gesture("browser.desktop", ELBOW).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(store.entries().len(), 1);
}

#[test]
fn full_favorites_can_gain_gestures_without_silently_removing_other_apps() {
    let mut store = FavoriteStore::in_memory();
    for i in 0..MAX_FAVORITES {
        store.add(&format!("favorite-{i}.desktop")).unwrap();
    }
    let before = store.entries().to_vec();
    assert!(store.set_gesture("editor.desktop", ELBOW).is_err());
    assert_eq!(store.entries(), before);
    store.set_gesture("favorite-0.desktop", ELBOW).unwrap();
    assert_eq!(store.entries().len(), MAX_FAVORITES);
    assert_eq!(&store.entries()[1..], &before[1..]);
    assert_eq!(
        store.recognize(ELBOW).unwrap(),
        Recognition::Match("favorite-0.desktop".into())
    );
}

#[test]
fn saving_editing_and_retargeting_gestures_preserves_desktop_favorites() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    store.add("browser.desktop").unwrap();
    store.add("editor.desktop").unwrap();
    store.add("files.desktop").unwrap();
    store.set_gesture("editor.desktop", ELBOW).unwrap();
    store.set_gesture("editor.desktop", LINE).unwrap();
    let saved = FavoriteStore::load(&path).unwrap();
    assert_eq!(saved.entries()[0].app_id, "browser.desktop");
    assert!(saved.entries()[0].gesture.is_none());
    assert_eq!(saved.entries()[1].app_id, "editor.desktop");
    assert_eq!(saved.entries()[2].app_id, "files.desktop");
    assert!(saved.entries()[2].gesture.is_none());
    assert_eq!(
        saved.recognize(LINE).unwrap(),
        Recognition::Match("editor.desktop".into())
    );
    store
        .replace_gesture_app("editor.desktop", "files.desktop", ELBOW)
        .unwrap();
    let saved = FavoriteStore::load(&path).unwrap();
    assert_eq!(
        saved
            .entries()
            .iter()
            .map(|entry| entry.app_id.as_str())
            .collect::<Vec<_>>(),
        ["browser.desktop", "editor.desktop", "files.desktop"]
    );
    assert!(saved.entries()[0].gesture.is_none());
    assert!(saved.entries()[1].gesture.is_none());
    assert_eq!(
        saved.recognize(ELBOW).unwrap(),
        Recognition::Match("files.desktop".into())
    );
}

#[test]
fn failed_upsert_preserves_previous_file_and_memory_including_legacy_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gestures.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    store.add("legacy.desktop").unwrap();
    let previous = store.entries().to_vec();
    let mut other = FavoriteStore::load(&path).unwrap();
    other.set_gesture("browser.desktop", LINE).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(store.set_gesture("editor.desktop", ELBOW).is_err());
    assert_eq!(store.entries(), previous);
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    assert_eq!(
        FavoriteStore::load(path).unwrap().recognize(LINE).unwrap(),
        Recognition::Match("browser.desktop".into())
    );
}

#[test]
fn retargeting_a_shortcut_is_atomic_and_preserves_other_apps_and_external_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gestures.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    store.set_gesture("editor.desktop", ELBOW).unwrap();
    store.set_gesture("browser.desktop", LINE).unwrap();
    let before = std::fs::read(&path).unwrap();
    for (target, points) in [
        ("browser.desktop", ELBOW),
        ("new.desktop", LINE),
        ("new.desktop", &[[0.0, 0.0]][..]),
    ] {
        assert!(
            store
                .replace_gesture_app("editor.desktop", target, points)
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    store
        .replace_gesture_app("editor.desktop", "new.desktop", ELBOW)
        .unwrap();
    let saved = FavoriteStore::load(&path).unwrap();
    assert_eq!(
        saved
            .entries()
            .iter()
            .map(|entry| entry.app_id.as_str())
            .collect::<Vec<_>>(),
        ["editor.desktop", "browser.desktop", "new.desktop"]
    );
    assert!(saved.entries()[0].gesture.is_none());
    assert_eq!(
        saved.recognize(ELBOW).unwrap(),
        Recognition::Match("new.desktop".into())
    );
    assert_eq!(
        saved.recognize(LINE).unwrap(),
        Recognition::Match("browser.desktop".into())
    );
    let mut other = FavoriteStore::load(&path).unwrap();
    other.remove("browser.desktop").unwrap();
    let external = std::fs::read(&path).unwrap();
    let previous = store.entries().to_vec();
    assert!(
        store
            .replace_gesture_app("new.desktop", "third.desktop", ELBOW)
            .is_err()
    );
    assert_eq!(store.entries(), previous);
    assert_eq!(std::fs::read(path).unwrap(), external);
}

#[test]
fn full_store_retarget_keeps_every_pin_and_rejects_a_new_target_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("favorites.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    for index in 0..MAX_FAVORITES {
        store.add(&format!("favorite-{index}.desktop")).unwrap();
    }
    store.set_gesture("favorite-0.desktop", ELBOW).unwrap();
    let entries = store.entries().to_vec();
    let before = std::fs::read(&path).unwrap();
    assert!(
        store
            .replace_gesture_app("favorite-0.desktop", "new.desktop", ELBOW)
            .is_err()
    );
    assert_eq!(store.entries(), entries);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    store
        .replace_gesture_app("favorite-0.desktop", "favorite-1.desktop", ELBOW)
        .unwrap();
    let saved = FavoriteStore::load(path).unwrap();
    assert_eq!(
        saved
            .entries()
            .iter()
            .map(|entry| &entry.app_id)
            .collect::<Vec<_>>(),
        entries
            .iter()
            .map(|entry| &entry.app_id)
            .collect::<Vec<_>>()
    );
    assert!(saved.entries()[0].gesture.is_none());
    assert_eq!(
        saved.recognize(ELBOW).unwrap(),
        Recognition::Match("favorite-1.desktop".into())
    );
}
