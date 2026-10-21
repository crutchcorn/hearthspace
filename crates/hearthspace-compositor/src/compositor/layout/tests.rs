use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn normal_restart_restores_viewport_and_distinct_application_slots() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut store = LayoutStore::load(Some(path.clone()));
    store.layout.x = -2700;
    store.layout.y = 490;
    store.layout.scale = 0.75;
    assert_eq!(store.claim(15, "foot".into()), None);
    assert_eq!(store.claim(99, "foot".into()), None);
    store.remember(15, CanvasPoint { x: -2300, y: 500 });
    store.remember(99, CanvasPoint { x: 800, y: 600 });
    store.save().unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let mut restarted = LayoutStore::load(Some(path.clone()));
    assert_eq!(
        restarted.viewport(),
        (CanvasPoint { x: -2700, y: 490 }, 0.75)
    );
    assert_eq!(
        restarted.claim(1, "foot".into()),
        Some(CanvasPoint { x: -2300, y: 500 })
    );
    assert_eq!(
        restarted.claim(2, "foot".into()),
        Some(CanvasPoint { x: 800, y: 600 })
    );
    assert_eq!(restarted.claim(3, "browser".into()), None);
    restarted.remember(1, CanvasPoint { x: 30, y: 40 });
    restarted.save().unwrap();
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    assert_eq!(
        LayoutStore::load(Some(path)).claim(8, "foot".into()),
        Some(CanvasPoint { x: 30, y: 40 })
    );
}

#[test]
fn invalid_oversized_or_future_layouts_fall_back_without_preventing_startup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    for data in [
        b"broken".to_vec(),
        vec![b' '; MAX_BYTES as usize + 1],
        br#"{"version":2,"x":5,"y":7,"scale":1,"placements":[]}"#.to_vec(),
        br#"{"version":1,"x":5,"y":7,"scale":30,"placements":[]}"#.to_vec(),
        br#"{"version":1,"x":2147483647,"y":7,"scale":1,"placements":[]}"#.to_vec(),
    ] {
        fs::write(&path, data).unwrap();
        assert_eq!(
            LayoutStore::load(Some(path.clone())).layout,
            Layout::default()
        );
    }
}

#[test]
fn disabled_persistence_never_claims_and_history_is_bounded() {
    let mut disabled = LayoutStore::load(None);
    assert_eq!(disabled.claim(1, "foot".into()), None);
    disabled.remember(1, CanvasPoint { x: 10, y: 10 });
    assert!(disabled.layout.placements.is_empty());
    let directory = tempfile::tempdir().unwrap();
    let mut store = LayoutStore::load(Some(directory.path().join("layout.json")));
    for id in 0..200 {
        store.claim(id, format!("app-{id}"));
        store.remember(id, CanvasPoint { x: id as i32, y: 0 });
    }
    assert_eq!(store.layout.placements.len(), MAX_PLACEMENTS);
    assert_eq!(store.layout.placements.first().unwrap().app_id, "app-72");
    store.save().unwrap();
    for id in 1000..1010 {
        store.claim(id, "foot".into());
    }
    assert_eq!(
        store.claimed.values().filter(|key| key.0 == "foot").count(),
        8
    );
}

#[test]
fn duplicate_slots_and_out_of_range_positions_are_rejected() {
    let item = Placement {
        app_id: "foot".into(),
        slot: 0,
        x: 10,
        y: 10,
    };
    let layout = Layout {
        placements: vec![item.clone(), item],
        ..Layout::default()
    };
    assert!(layout.validate().is_err());
    let directory = tempfile::tempdir().unwrap();
    let mut store = LayoutStore::load(Some(directory.path().join("layout.json")));
    store.claim(1, "foot".into());
    store.remember(1, CanvasPoint { x: i32::MIN, y: 0 });
    assert!(store.layout.placements.is_empty());
}

#[test]
fn desktop_positions_survive_restart_independently_of_window_slots() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut store = LayoutStore::load(Some(path.clone()));
    store.remember_desktop("app:foot.desktop", CanvasPoint { x: -560, y: 840 });
    store.remember_desktop("file:6e6f746573", CanvasPoint { x: 128, y: 256 });
    store.save().unwrap();
    let mut restarted = LayoutStore::load(Some(path));
    assert_eq!(
        restarted.desktop_position("app:foot.desktop"),
        Some(CanvasPoint { x: -560, y: 840 })
    );
    assert_eq!(
        restarted.desktop_position("file:6e6f746573"),
        Some(CanvasPoint { x: 128, y: 256 })
    );
    assert_eq!(restarted.claim(1, "foot".into()), None);
    restarted.remember_desktop("bad", CanvasPoint { x: i32::MAX, y: 0 });
    assert_eq!(restarted.desktop_position("bad"), None);
}

#[test]
fn old_layouts_without_desktop_items_remain_loadable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    fs::write(
        &path,
        br#"{"version":1,"x":40,"y":50,"scale":1,"placements":[]}"#,
    )
    .unwrap();
    let store = LayoutStore::load(Some(path));
    assert_eq!(store.viewport().0, CanvasPoint { x: 40, y: 50 });
    assert!(store.layout.desktop.is_empty());
}

#[test]
fn maximum_desktop_catalog_survives_restart_with_json_escaped_ids() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("layout.json");
    let mut store = LayoutStore::load(Some(path.clone()));
    for index in 0..MAX_DESKTOP_ITEMS {
        let prefix = format!("app:{index:04}:");
        let id = format!("{prefix}{}", "\\".repeat(1024 - prefix.len()));
        let coordinate = i32::try_from(index).unwrap();
        store.remember_desktop(
            &id,
            CanvasPoint {
                x: coordinate,
                y: -coordinate,
            },
        );
    }
    store.save().unwrap();
    assert!(fs::metadata(&path).unwrap().len() > 4 * 1024 * 1024);
    let restarted = LayoutStore::load(Some(path));
    assert_eq!(restarted.layout.desktop, store.layout.desktop);
}
