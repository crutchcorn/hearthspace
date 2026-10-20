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
fn obsolete_empty_favorites_do_not_consume_gesture_capacity() {
    let mut store = FavoriteStore::in_memory();
    for i in 0..MAX_FAVORITES {
        store.add(&format!("legacy-{i}.desktop")).unwrap();
    }
    store.set_gesture("editor.desktop", ELBOW).unwrap();
    assert_eq!(store.entries().len(), 1);
    assert_eq!(store.entries()[0].app_id, "editor.desktop");
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
    assert_eq!(saved.entries().len(), 2);
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
