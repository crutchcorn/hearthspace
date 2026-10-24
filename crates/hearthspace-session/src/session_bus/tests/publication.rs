use super::*;
use std::{fs, thread};

#[test]
fn shared_publication_reaches_real_activation_and_unsets_invalid_manager_values() {
    let bus = Bus::start();
    let paths = bus.paths("shared");
    bus.seed(pairs(&[("DISPLAY", ":92")]).into_iter().collect());
    bus.state.lock().unwrap().values.extend(pairs(&[
        ("AT_SPI_BUS_ADDRESS", "unix:path=/synthetic/stale-atspi"),
        ("WAYLAND_SOCKET", "99"),
    ]));
    let mut session = bus.session(&paths);
    session.vars.extend(pairs(&[
        ("RUST_LOG", "trace"),
        ("ARBITRARY_TEST_VALUE", "excluded"),
        ("XDG_CONFIG_HOME", "/synthetic/private/config"),
        ("GSETTINGS_BACKEND", "keyfile"),
        ("AT_SPI_BUS_ADDRESS", "unix:path=/synthetic/stale-atspi"),
        ("WAYLAND_SOCKET", "99"),
    ]));
    let mut guard = update_activation_environment(
        &paths,
        &session,
        false,
        Instant::now() + Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .unwrap()
    .unwrap();
    let manager = bus.values();
    let activated = bus.probe();
    assert_eq!(
        manager["XDG_RUNTIME_DIR"],
        paths.host_runtime_dir.to_str().unwrap()
    );
    assert_eq!(
        manager["WAYLAND_DISPLAY"],
        paths.session_dir.join("wayland-test").to_str().unwrap()
    );
    assert_eq!(manager["DISPLAY"], "");
    assert!(!manager[cleanup::MARKER].is_empty());
    for key in [
        "XDG_RUNTIME_DIR",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "PATH",
        "XDG_DATA_DIRS",
        "XDG_CONFIG_DIRS",
        "GTK_USE_PORTAL",
        "HEARTHSPACE_SETTINGS_SOCKET",
        "HEARTHSPACE_SESSION_SOCKET",
        "HEARTHSPACE_COMMAND_SOCKET",
        "HEARTHSPACE_SESSION_DIR",
        "HEARTHSPACE_HOST_XDG_RUNTIME_DIR",
        "HEARTHSPACE_SESSION_BUS_ADDRESS",
        "HEARTHSPACE_APP_SCOPE_UNIT",
        "HEARTHSPACE_DEV",
        "HEARTHSPACE_REPO_ROOT",
        cleanup::MARKER,
    ] {
        assert_eq!(
            activated[key].as_ref(),
            manager.get(key),
            "activation differs for {key}"
        );
    }
    for key in [
        "AT_SPI_BUS_ADDRESS",
        "WAYLAND_SOCKET",
        "RUST_LOG",
        "ARBITRARY_TEST_VALUE",
        "XDG_CONFIG_HOME",
        "GSETTINGS_BACKEND",
    ] {
        assert!(!manager.contains_key(key), "manager contains {key}");
        assert_eq!(activated[key], None, "daemon imported {key}");
    }
    assert_eq!(
        bus.state.lock().unwrap().set_calls.len(),
        1,
        "real updater must reach systemd"
    );
    guard.retire().unwrap();
}

#[test]
fn private_update_never_reads_or_changes_the_user_manager() {
    let bus = Bus::start();
    let paths = bus.paths("private");
    let before: BTreeMap<_, _> = pairs(&[("DISPLAY", ":93"), ("WAYLAND_SOCKET", "99")])
        .into_iter()
        .collect();
    {
        let mut state = bus.state.lock().unwrap();
        state.values = before.clone();
        state.stall_reads = true;
    }
    let profile = pairs(&[
        ("XDG_CONFIG_HOME", "/synthetic/private/config"),
        ("GSETTINGS_BACKEND", "keyfile"),
    ]);
    let values = environment::values(&paths, &bus.session(&paths), true, &profile).unwrap();
    // The profile payload is constructed explicitly: for_session reads ambient env.
    cleanup::operations::update(
        &bus.address,
        &values,
        false,
        Instant::now() + Duration::from_secs(2),
        &AtomicBool::new(false),
    )
    .unwrap();
    let activated = bus.probe();
    for key in [
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "GSETTINGS_BACKEND",
        "DISPLAY",
    ] {
        assert_eq!(activated[key].as_ref(), values.get(key));
    }
    let state = bus.state.lock().unwrap();
    assert_eq!(state.values, before);
    assert_eq!(state.reads, 0);
    assert!(state.set_calls.is_empty());
    assert!(state.replace_calls.is_empty());
    assert!(!paths.session_dir.join(cleanup::RECORD).exists());
}

#[test]
fn failure_after_the_bus_half_update_rolls_back_both_activation_targets() {
    let bus = Bus::start();
    let paths = bus.paths("failed");
    let baseline: BTreeMap<_, _> = pairs(&[
        ("DISPLAY", ":94"),
        ("PATH", "/usr/bin:/bin"),
        ("WAYLAND_DISPLAY", "original-wayland"),
    ])
    .into_iter()
    .collect();
    bus.seed(baseline.clone());
    let (release, gate) = tokio::sync::oneshot::channel();
    {
        let mut state = bus.state.lock().unwrap();
        state.fail_next_set = true;
        state.set_gate = Some(gate);
    }
    thread::scope(|scope| {
        let worker = scope.spawn(|| {
            update_activation_environment(
                &paths,
                &bus.session(&paths),
                false,
                Instant::now() + Duration::from_secs(5),
                &AtomicBool::new(false),
            )
        });
        fixture::wait_until(|| !bus.state.lock().unwrap().set_calls.is_empty());
        let halfway = bus.probe();
        assert_eq!(
            halfway["WAYLAND_DISPLAY"].as_deref(),
            paths.session_dir.join("wayland-test").to_str()
        );
        assert_eq!(bus.values()["WAYLAND_DISPLAY"], "original-wayland");
        release.send(()).unwrap();
        assert!(worker.join().unwrap().is_err());
    });
    assert_eq!(bus.values(), baseline);
    let restored = bus.probe();
    for (key, value) in baseline {
        assert_eq!(restored[&key].as_ref(), Some(&value));
    }
    assert!(!paths.session_dir.join(cleanup::RECORD).exists());
}

#[test]
fn failed_rollback_keeps_a_receipt_that_standalone_retirement_can_retry() {
    let bus = Bus::start();
    let paths = bus.paths("retry");
    {
        let mut state = bus.state.lock().unwrap();
        state.fail_next_set = true;
        state.fail_replace_after = Some(1); // Claim succeeds; rollback fails.
    }
    let result = update_activation_environment(
        &paths,
        &bus.session(&paths),
        false,
        Instant::now() + Duration::from_secs(5),
        &AtomicBool::new(false),
    );
    assert!(result.is_err());
    assert!(paths.session_dir.join(cleanup::RECORD).is_file());
    assert!(!bus.values()[cleanup::MARKER].is_empty());
    bus.state.lock().unwrap().fail_replace_after = None;
    cleanup::retire_from_session_dir(&paths.session_dir, None).unwrap();
    assert!(bus.values().is_empty());
    assert!(!fs::exists(paths.session_dir.join(cleanup::RECORD)).unwrap());
}
