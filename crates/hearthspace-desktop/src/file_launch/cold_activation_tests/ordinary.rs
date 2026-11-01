use super::*;

fn activation_only_fixture() -> Fixture {
    let fixture = fixture();
    fs::write(
        fixture
            .path()
            .join(format!("share/applications/{APP}.desktop")),
        "[Desktop Entry]\nType=Application\nName=Cold Player\nDBusActivatable=true\nPrefersNonDefaultGPU=true\n",
    )
    .unwrap();
    fixture
}

#[test]
fn activation_only_application_receives_one_cold_and_one_warm_activate() {
    let fixture = activation_only_fixture();
    let bus = Bus::start_with_services(fixture.path());
    let _service = ServiceLifetime(fixture.path());
    let launch = || {
        assert_success(
            fixture
                .child(LAUNCH)
                .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address)
                .env("HEARTHSPACE_TEST_LAUNCH_MODE", "activate"),
        );
    };
    launch();
    assert_eq!(
        fs::read_to_string(fixture.path().join("activations")).unwrap(),
        "1"
    );
    launch();
    assert_eq!(
        fs::read_to_string(fixture.path().join("activations")).unwrap(),
        "2"
    );
    assert!(fixture.path().join("broker-started").exists());
    assert!(!fixture.path().join("ready").exists());
}

#[test]
fn activation_only_application_rejection_is_reported_without_retry() {
    let fixture = activation_only_fixture();
    fs::write(fixture.path().join("reject-open"), []).unwrap();
    let bus = Bus::start_with_services(fixture.path());
    let _service = ServiceLifetime(fixture.path());
    assert_success(
        fixture
            .child(LAUNCH)
            .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address)
            .env("HEARTHSPACE_TEST_LAUNCH_MODE", "activate-rejected"),
    );
    assert_eq!(
        fs::read_to_string(fixture.path().join("activations")).unwrap(),
        "1"
    );
    assert!(!fixture.path().join("ready").exists());
}

#[test]
fn saved_nested_activation_only_entry_keeps_its_cold_activation_identity() {
    let fixture = activation_only_fixture();
    let root = fixture.path();
    let source = root.join(format!("system/applications/vendor/{APP}.desktop"));
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::rename(
        root.join(format!("share/applications/{APP}.desktop")),
        &source,
    )
    .unwrap();
    let original = fs::read_to_string(&source)
        .unwrap()
        .replace("PrefersNonDefaultGPU=true", "PrefersNonDefaultGPU=false");
    fs::write(&source, &original).unwrap();
    let bus = Bus::start_with_services(root);
    let _service = ServiceLifetime(root);
    assert!(!root.join("broker-started").exists());
    assert_success(
        fixture
            .child("file_launch::cold_activation_tests::ordinary::save_and_activate_nested_entry")
            .env("XDG_DATA_DIRS", root.join("system"))
            .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &bus.address),
    );
    assert_eq!(fs::read_to_string(root.join("activations")).unwrap(), "1");
    assert!(root.join("broker-started").exists());
    assert!(
        !root.join("ready").exists(),
        "activation-only app has no Exec fallback"
    );
    assert_eq!(fs::read_to_string(source).unwrap(), original);
    assert!(
        root.join(format!("share/applications/vendor/{APP}.desktop"))
            .is_file()
    );
    assert!(
        !root
            .join(format!("share/applications/vendor-{APP}.desktop"))
            .exists()
    );
}

#[test]
fn save_and_activate_nested_entry() {
    let Some(root) = child_root() else { return };
    let id = format!("vendor-{APP}.desktop");
    let before = installed_application(&id).unwrap();
    assert_eq!(
        before.desktop_file,
        root.join(format!("system/applications/vendor/{APP}.desktop"))
    );
    assert!(!before.prefers_non_default_gpu);
    let saved = crate::app_catalog::set_app_prefers_non_default_gpu(&id, true).unwrap();
    assert_eq!(saved.id, id);
    assert_eq!(
        saved.path,
        root.join(format!("share/applications/vendor/{APP}.desktop"))
    );
    assert!(saved.exec.is_empty());
    assert!(saved.dbus_activatable);
    assert!(saved.prefers_non_default_gpu);
    let app = installed_application(&id).unwrap();
    assert_eq!(app.desktop_file, saved.path);
    // Exercise the real native GIO helper and private broker after persistence;
    // a flattened basename would request a different, nonexistent bus service.
    let mut command = native_gio_command(
        &app.desktop_file,
        &[],
        "wayland-native",
        &[],
        &root.join("helper"),
    )
    .unwrap();
    command.env(
        "HEARTHSPACE_TEST_EXECUTABLE",
        std::env::current_exe().unwrap(),
    );
    run_opener(command).unwrap();
}
