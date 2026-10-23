use super::*;
use std::fs;

#[test]
fn retirement_restores_baseline_removes_absent_keys_and_neutralizes_daemon_paths() {
    let bus = Bus::start();
    let paths = bus.paths("retire");
    let baseline: BTreeMap<_, _> = pairs(&[
        ("DISPLAY", ":95"),
        ("WAYLAND_DISPLAY", "original-wayland"),
        ("PATH", "/usr/bin:/bin"),
        ("XDG_CONFIG_DIRS", "/etc/xdg"),
    ])
    .into_iter()
    .collect();
    bus.seed(baseline.clone());
    let mut guard = bus.publish(&paths);
    guard.retire().unwrap();
    guard.retire().unwrap();
    assert_eq!(bus.values(), baseline);
    let activated = bus.probe();
    for (key, value) in &baseline {
        assert_eq!(activated[key].as_ref(), Some(value));
    }
    assert_eq!(
        activated["HEARTHSPACE_SETTINGS_SOCKET"].as_deref(),
        Some("")
    );
    assert_eq!(
        activated["XDG_RUNTIME_DIR"].as_deref(),
        paths.host_runtime_dir.to_str()
    );
    let daemon_values = activated
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
    fixture::assert_no_session_paths(&daemon_values, &paths.session_dir);
    assert!(!paths.session_dir.join(cleanup::RECORD).exists());
}

#[test]
fn retiring_an_older_owner_preserves_newer_values_and_newer_retirement_restores_original() {
    let bus = Bus::start();
    let first = bus.paths("first");
    let second = bus.paths("second");
    let baseline: BTreeMap<_, _> =
        pairs(&[("PATH", "/usr/bin:/bin"), ("WAYLAND_DISPLAY", "original")])
            .into_iter()
            .collect();
    bus.seed(baseline.clone());
    let mut older = bus.publish(&first);
    let first_marker = bus.values()[cleanup::MARKER].clone();
    let mut newer = bus.publish(&second);
    let current = bus.values();
    assert_ne!(current[cleanup::MARKER], first_marker);
    older.retire().unwrap();
    assert_eq!(bus.values(), current);
    assert!(second.session_dir.join(cleanup::RECORD).is_file());
    let activated = bus.probe();
    assert_eq!(
        activated["WAYLAND_DISPLAY"].as_ref(),
        current.get("WAYLAND_DISPLAY")
    );
    newer.retire().unwrap();
    assert_eq!(bus.values(), baseline);
    fixture::assert_no_session_paths(&bus.values(), &first.session_dir);
}

#[test]
fn old_guard_preserves_new_receipt_when_directory_and_published_values_are_identical() {
    let bus = Bus::start();
    let paths = bus.paths("reused");
    let mut older = bus.publish(&paths);
    let original = bus.values();
    // A crashed lifecycle loses its receipt while an old captured guard remains.
    fs::remove_file(paths.session_dir.join(cleanup::RECORD)).unwrap();
    let mut newer = bus.publish(&paths);
    let current = bus.values();
    assert_ne!(original[cleanup::MARKER], current[cleanup::MARKER]);
    for (key, value) in &original {
        if key != cleanup::MARKER {
            assert_eq!(current.get(key), Some(value));
        }
    }
    let receipt = fs::read(paths.session_dir.join(cleanup::RECORD)).unwrap();
    older.retire().unwrap();
    assert_eq!(bus.values(), current);
    assert_eq!(
        fs::read(paths.session_dir.join(cleanup::RECORD)).unwrap(),
        receipt
    );
    newer.retire().unwrap();
    fixture::assert_no_session_paths(&bus.values(), &paths.session_dir);
}

#[test]
fn standalone_retirement_replays_receipt_and_preserves_independently_changed_values() {
    let bus = Bus::start();
    let paths = bus.paths("standalone");
    let mut guard = bus.publish(&paths);
    bus.state
        .lock()
        .unwrap()
        .values
        .insert("PATH".into(), "/synthetic/newer/bin".into());
    cleanup::retire_from_session_dir(&paths.session_dir, None).unwrap();
    assert_eq!(
        bus.values().get("PATH").map(String::as_str),
        Some("/synthetic/newer/bin")
    );
    assert!(!bus.values().contains_key(cleanup::MARKER));
    assert_eq!(bus.probe()["PATH"].as_deref(), Some("/synthetic/newer/bin"));
    guard.retire().unwrap();
}

#[test]
fn same_owner_edit_during_second_snapshot_is_not_overwritten_by_baseline() {
    let bus = Bus::start();
    let paths = bus.paths("concurrent");
    bus.seed(pairs(&[("PATH", "/usr/bin:/bin")]).into_iter().collect());
    let mut guard = bus.publish(&paths);
    {
        let mut state = bus.state.lock().unwrap();
        state.change_on_read = Some((
            state.reads + 2,
            pairs(&[("PATH", "/synthetic/concurrent/bin")])
                .into_iter()
                .collect(),
        ));
    }
    guard.retire().unwrap();
    assert_eq!(bus.values()["PATH"], "/synthetic/concurrent/bin");
}
