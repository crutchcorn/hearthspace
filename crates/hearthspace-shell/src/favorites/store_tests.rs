use super::*;

const CHECK: &[Point] = &[[0.0, 70.0], [40.0, 130.0], [170.0, 0.0]];
const ELBOW: &[Point] = &[[0.0, 0.0], [0.0, 150.0], [150.0, 150.0]];

fn populated() -> FavoriteStore {
    let mut store = FavoriteStore::in_memory();
    store.add("editor.desktop").unwrap();
    store.assign("editor.desktop", CHECK).unwrap();
    store
}

#[test]
fn favorites_can_be_added_assigned_replaced_cleared_and_removed() {
    let mut store = FavoriteStore::in_memory();
    store.add("editor.desktop").unwrap();
    store.add("editor.desktop").unwrap();
    assert_eq!(store.entries().len(), 1);
    assert_eq!(store.recognize(CHECK).unwrap(), Recognition::NoMatch);
    store.assign("editor.desktop", CHECK).unwrap();
    assert_eq!(
        store.recognize(CHECK).unwrap(),
        Recognition::Match("editor.desktop".into())
    );
    store.assign("editor.desktop", ELBOW).unwrap();
    assert_eq!(store.recognize(CHECK).unwrap(), Recognition::NoMatch);
    assert_eq!(
        store.recognize(ELBOW).unwrap(),
        Recognition::Match("editor.desktop".into())
    );
    store.clear_gesture("editor.desktop").unwrap();
    assert_eq!(store.recognize(ELBOW).unwrap(), Recognition::NoMatch);
    store.remove("editor.desktop").unwrap();
    assert!(store.entries().is_empty());
    assert!(store.assign("removed.desktop", CHECK).is_err());
}

#[test]
fn duplicate_and_confusable_assignments_are_rejected_without_losing_old_shapes() {
    let mut store = populated();
    store.add("browser.desktop").unwrap();
    store.assign("browser.desktop", ELBOW).unwrap();
    let previous = store.entries().to_vec();
    assert!(
        store
            .assign("browser.desktop", CHECK)
            .unwrap_err()
            .to_string()
            .contains("editor.desktop")
    );
    let variant = [[9.0, 65.0], [45.0, 125.0], [174.0, 2.0]];
    assert!(store.assign("browser.desktop", &variant).is_err());
    assert_eq!(store.entries(), previous);
}

#[test]
fn ambiguous_imported_shapes_never_launch_arbitrarily() {
    let mut store = populated();
    let mut duplicate = store.entries[0].clone();
    duplicate.app_id = "browser.desktop".into();
    store.entries.push(duplicate);
    assert_eq!(store.recognize(CHECK).unwrap(), Recognition::Ambiguous);
    store.entries.reverse();
    assert_eq!(store.recognize(CHECK).unwrap(), Recognition::Ambiguous);
}

#[test]
fn a_stroke_between_two_valid_assignments_is_ambiguous() {
    let mut store = populated();
    store.add("line.desktop").unwrap();
    store
        .assign("line.desktop", &[[0.0, 0.0], [160.0, 0.0]])
        .unwrap();
    // Both templates are individually clear. The half-height check is close
    // enough to either, so favor neither app even though one is slightly nearer.
    assert_eq!(
        store.recognize(CHECK).unwrap(),
        Recognition::Match("editor.desktop".into())
    );
    assert_eq!(
        store.recognize(&[[0.0, 0.0], [160.0, 0.0]]).unwrap(),
        Recognition::Match("line.desktop".into())
    );
    assert_eq!(
        store
            .recognize(&[[0.0, 35.0], [40.0, 65.0], [170.0, 0.0]])
            .unwrap(),
        Recognition::Ambiguous
    );
}

#[test]
fn independent_shapes_resolve_deterministically_and_unrelated_input_is_rejected() {
    let mut store = populated();
    store.add("browser.desktop").unwrap();
    store.assign("browser.desktop", ELBOW).unwrap();
    for _ in 0..10 {
        assert_eq!(
            store.recognize(CHECK).unwrap(),
            Recognition::Match("editor.desktop".into())
        );
        assert_eq!(
            store.recognize(ELBOW).unwrap(),
            Recognition::Match("browser.desktop".into())
        );
        assert_eq!(
            store.recognize(&[[0.0, 0.0], [150.0, 0.0]]).unwrap(),
            Recognition::NoMatch
        );
    }
}

#[test]
fn limits_and_invalid_ids_do_not_mutate_the_store() {
    let mut store = FavoriteStore::in_memory();
    for id in ["", "bad\napp", &"x".repeat(513)] {
        assert!(store.add(id).is_err());
    }
    for i in 0..MAX_FAVORITES {
        store.add(&format!("app-{i}.desktop")).unwrap();
    }
    assert!(store.add("extra.desktop").is_err());
    assert_eq!(store.entries().len(), MAX_FAVORITES);
}

#[test]
fn persistent_mutations_roundtrip_with_private_atomic_files() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config/hearthspace/favorites.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    assert!(!path.exists());
    store.add("editor.desktop").unwrap();
    store.assign("editor.desktop", CHECK).unwrap();
    let loaded = FavoriteStore::load(&path).unwrap();
    assert_eq!(loaded.entries()[0].app_id, "editor.desktop");
    assert_eq!(
        loaded.recognize(CHECK).unwrap(),
        Recognition::Match("editor.desktop".into())
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
    store.clear_gesture("editor.desktop").unwrap();
    assert!(
        FavoriteStore::load(&path).unwrap().entries()[0]
            .gesture
            .is_none()
    );
    store.remove("editor.desktop").unwrap();
    assert!(FavoriteStore::load(path).unwrap().entries().is_empty());
}

#[test]
fn malformed_unknown_version_duplicate_and_oversize_files_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.json");
    for text in ["not json".to_owned(),
        r#"{"version":2,"favorites":[]}"#.into(),
        r#"{"version":1,"favorites":[{"app_id":"a","gesture":null},{"app_id":"a","gesture":null}]}"#.into(),
        " ".repeat(256 * 1024 + 1),
    ] {
        std::fs::write(&path, &text).unwrap();
        assert!(FavoriteStore::load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }
}

#[test]
fn failed_save_preserves_memory_and_existing_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    store.add("editor.desktop").unwrap();
    let previous = store.entries().to_vec();
    let backup = directory.path().join("backup.json");
    std::fs::rename(&path, &backup).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(store.add("browser.desktop").is_err());
    assert_eq!(store.entries(), previous);
    assert_eq!(FavoriteStore::load(backup).unwrap().entries(), previous);
    assert!(path.is_dir());
}

#[test]
fn failed_directory_creation_does_not_add_a_favorite() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("config");
    let path = parent.join("favorites.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    std::fs::write(&parent, "existing user data").unwrap();
    assert!(store.add("editor.desktop").is_err());
    assert!(store.entries().is_empty());
    assert_eq!(
        std::fs::read_to_string(parent).unwrap(),
        "existing user data"
    );
}

#[test]
fn stale_store_cannot_overwrite_another_shells_changes_or_a_deleted_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.json");
    let mut store = FavoriteStore::load(&path).unwrap();
    store.add("editor.desktop").unwrap();
    let mut other = FavoriteStore::load(&path).unwrap();
    other.add("browser.desktop").unwrap();
    assert!(store.remove("editor.desktop").is_err());
    assert_eq!(
        FavoriteStore::load(&path).unwrap().entries(),
        other.entries()
    );
    std::fs::remove_file(&path).unwrap();
    assert!(other.add("terminal.desktop").is_err());
    assert!(!path.exists());
}

#[test]
fn reload_observes_settings_edits_but_preserves_known_data_on_parse_errors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("favorites.json");
    let mut settings = FavoriteStore::load(&path).unwrap();
    settings.add("editor.desktop").unwrap();
    let mut shell = FavoriteStore::load(&path).unwrap();
    assert!(!shell.reload_if_changed().unwrap());
    settings.add("browser.desktop").unwrap();
    assert!(shell.reload_if_changed().unwrap());
    assert_eq!(shell.entries(), settings.entries());
    assert!(!shell.reload_if_changed().unwrap());
    std::fs::write(&path, "malformed replacement").unwrap();
    assert!(shell.reload_if_changed().is_err());
    assert_eq!(shell.entries(), settings.entries());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "malformed replacement"
    );
    std::fs::remove_file(&path).unwrap();
    assert!(shell.reload_if_changed().unwrap());
    assert!(shell.entries().is_empty());
    let mut memory = populated();
    assert!(!memory.reload_if_changed().unwrap());
    assert_eq!(memory.entries().len(), 1);
}
