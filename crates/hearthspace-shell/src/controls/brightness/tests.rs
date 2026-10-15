use super::{
    logind, sysfs,
    test_support::{Bus, Logind, device},
};

#[tokio::test]
async fn absent_backlight_is_optional_without_a_logind_service() {
    let dir = tempfile::tempdir().unwrap();
    let bus = Bus::start();
    assert!(bus.backend(dir.path()).read().await.unwrap().is_none());
    assert!(
        bus.backend(&dir.path().join("missing"))
            .read()
            .await
            .unwrap()
            .is_none()
    );
    assert!(bus.backend(dir.path()).set(50).await.is_err());
}

#[tokio::test]
async fn discovery_prefers_firmware_then_platform_then_raw_and_recovers_from_hotplug() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "z_raw", "raw", 255, 127);
    device(dir.path(), "a_raw", "raw", 100, 60);
    device(dir.path(), "platform", "platform", 100, 70);
    device(dir.path(), "firmware", "firmware", 100, 80);
    let bus = Bus::start();
    let _service = Logind::start(&bus, dir.path()).await;
    let backend = bus.backend(dir.path());
    for (name, percentage) in [
        ("firmware", 80),
        ("platform", 70),
        ("a_raw", 60),
        ("z_raw", 50),
    ] {
        let status = backend.read().await.unwrap().unwrap();
        assert_eq!(
            (status.device.as_str(), status.percentage),
            (name, percentage)
        );
        std::fs::remove_dir_all(dir.path().join(name)).unwrap();
    }
    assert!(backend.read().await.unwrap().is_none());
    device(dir.path(), "new", "raw", 100, 35);
    assert_eq!(backend.read().await.unwrap().unwrap().percentage, 35);
}

#[tokio::test]
async fn invalid_devices_are_not_reported_as_zero_brightness() {
    let dir = tempfile::tempdir().unwrap();
    for (maximum, current) in [(0, 0), (10, 11)] {
        device(dir.path(), "panel", "raw", maximum, current);
        assert!(sysfs::discover(dir.path()).await.is_err());
    }
    device(dir.path(), "panel", "invalid", 100, 70);
    assert!(sysfs::discover(dir.path()).await.is_err());
    device(dir.path(), "good", "raw", 100, 50);
    assert_eq!(
        sysfs::discover(dir.path()).await.unwrap().unwrap().name,
        "good"
    );
    assert!(sysfs::read(dir.path(), "../outside").await.is_err());
}

#[tokio::test]
async fn percentages_handle_large_ranges_and_keep_a_nonzero_floor() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", u32::MAX, u32::MAX);
    let panel = sysfs::read(dir.path(), "panel").await.unwrap();
    assert_eq!(panel.status().percentage, 100);
    assert_eq!(panel.level(100), u32::MAX);
    assert_eq!(panel.level(0), 42_949_673);
    device(dir.path(), "panel", "raw", 10, 0);
    let panel = sysfs::read(dir.path(), "panel").await.unwrap();
    assert_eq!(panel.status().percentage, 0);
    assert_eq!(panel.level(0), 1);
    assert_eq!(panel.level(1), 1);
}

#[tokio::test]
async fn writes_use_only_logind_and_return_the_drivers_rounded_level() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", 15, 8);
    let bus = Bus::start();
    let service = Logind::start(&bus, dir.path()).await;
    let backend = bus.backend(dir.path());
    assert_eq!(backend.read().await.unwrap().unwrap().percentage, 53);
    assert_eq!(backend.set(50).await.unwrap().percentage, 53);
    assert_eq!(backend.set(0).await.unwrap().percentage, 7);
    assert!(backend.set(101).await.is_err());
    let state = service.state.lock().unwrap();
    assert_eq!(
        state.requests,
        vec![
            ("backlight".into(), "panel".into(), 8),
            ("backlight".into(), "panel".into(), 1)
        ]
    );
    assert_eq!(state.pids, vec![std::process::id(); 3]);
}

#[tokio::test]
async fn nested_shell_never_contacts_logind_or_changes_host_brightness() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", 100, 60);
    let bus = Bus::start();
    let service = Logind::start(&bus, dir.path()).await;
    let mut backend = bus.backend(dir.path());
    backend.native = false;
    assert!(backend.read().await.is_err());
    assert!(backend.set(50).await.is_err());
    assert!(service.state.lock().unwrap().pids.is_empty());
    assert_eq!(
        sysfs::read(dir.path(), "panel")
            .await
            .unwrap()
            .status()
            .percentage,
        60
    );
}

#[tokio::test]
async fn inactive_remote_and_seatless_sessions_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", 100, 60);
    let bus = Bus::start();
    let service = Logind::start(&bus, dir.path()).await;
    let backend = bus.backend(dir.path());
    for invalid in 0..4 {
        {
            let mut state = service.state.lock().unwrap();
            state.active = invalid != 0;
            state.remote = invalid == 1;
            state.seat = if invalid == 2 { "" } else { "seat0" }.into();
            state.vt = if invalid == 3 { 0 } else { 7 };
        }
        assert!(backend.read().await.is_err());
        assert!(backend.set(50).await.is_err());
    }
    assert!(service.state.lock().unwrap().requests.is_empty());
}

#[tokio::test]
async fn foreground_permission_is_rechecked_without_a_property_cache() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", 100, 60);
    let bus = Bus::start();
    let service = Logind::start(&bus, dir.path()).await;
    let session = logind::Session::connect(Some(&bus.address)).await.unwrap();
    service.state.lock().unwrap().active = false;
    assert!(session.set("panel", 50).await.is_err());
    assert!(service.state.lock().unwrap().requests.is_empty());
}

#[tokio::test]
async fn replaced_logind_is_rejected_and_a_new_poll_recovers() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", 100, 60);
    let bus = Bus::start();
    let first = Logind::start(&bus, dir.path()).await;
    let session = logind::Session::connect(Some(&bus.address)).await.unwrap();
    first.release_name().await;
    let second = Logind::start(&bus, dir.path()).await;
    assert!(session.set("panel", 50).await.is_err());
    assert!(first.state.lock().unwrap().requests.is_empty());
    assert!(second.state.lock().unwrap().requests.is_empty());
    assert_eq!(
        bus.backend(dir.path()).set(70).await.unwrap().percentage,
        70
    );
    assert_eq!(second.state.lock().unwrap().requests.len(), 1);
}

#[tokio::test]
async fn logind_rejection_is_not_retried_or_reported_as_success() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", 100, 60);
    let bus = Bus::start();
    let service = Logind::start(&bus, dir.path()).await;
    service.state.lock().unwrap().reject = true;
    let backend = bus.backend(dir.path());
    assert!(
        backend
            .set(50)
            .await
            .unwrap_err()
            .contains("permission was denied")
    );
    assert_eq!(backend.read().await.unwrap().unwrap().percentage, 60);
    assert_eq!(service.state.lock().unwrap().requests.len(), 1);
}

#[tokio::test]
async fn stalled_logind_is_bounded_and_never_reaches_the_write() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", 100, 60);
    let bus = Bus::start();
    let service = Logind::start(&bus, dir.path()).await;
    service.state.lock().unwrap().stalled = true;
    let start = std::time::Instant::now();
    let mut backend = bus.backend(dir.path());
    backend.timeout = std::time::Duration::from_millis(100);
    assert!(backend.set(50).await.is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert!(service.state.lock().unwrap().requests.is_empty());
}

#[tokio::test]
async fn rejected_compositor_guard_after_discovery_never_reaches_the_write() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "panel", "raw", 100, 60);
    let bus = Bus::start();
    let service = Logind::start(&bus, dir.path()).await;
    let error = bus
        .backend(dir.path())
        .set_guarded(50, || async {
            assert!(!service.state.lock().unwrap().pids.is_empty());
            Err("The desktop was locked during discovery.".into())
        })
        .await
        .unwrap_err();
    assert!(error.contains("locked during discovery"));
    assert!(service.state.lock().unwrap().requests.is_empty());
    assert_eq!(
        sysfs::read(dir.path(), "panel")
            .await
            .unwrap()
            .status()
            .percentage,
        60
    );
}

#[tokio::test]
async fn relative_change_pins_its_baseline_target_and_readback_across_hotplug() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "original", "raw", 100, 60);
    let bus = Bus::start();
    let service = Logind::start(&bus, dir.path()).await;
    let backend = bus.backend(dir.path());
    let result = backend
        .step_guarded(5, || async {
            device(dir.path(), "preferred", "firmware", 100, 20);
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(result.device, "original");
    assert_eq!(result.percentage, 65);
    assert_eq!(
        service.state.lock().unwrap().requests,
        vec![("backlight".into(), "original".into(), 65)]
    );
    let current = backend.read().await.unwrap().unwrap();
    assert_eq!(current.device, "preferred");
    assert_eq!(current.percentage, 20);
}

#[tokio::test]
async fn relative_change_clamps_large_steps_and_never_transfers_to_a_replacement() {
    let dir = tempfile::tempdir().unwrap();
    device(dir.path(), "original", "raw", 100, 60);
    let bus = Bus::start();
    let _service = Logind::start(&bus, dir.path()).await;
    let backend = bus.backend(dir.path());
    assert_eq!(
        backend
            .step_guarded(i8::MAX, || async { Ok(()) })
            .await
            .unwrap()
            .percentage,
        100
    );
    assert_eq!(
        backend
            .step_guarded(i8::MIN, || async { Ok(()) })
            .await
            .unwrap()
            .percentage,
        1
    );
    assert!(
        backend
            .step_guarded(5, || async {
                std::fs::remove_dir_all(dir.path().join("original")).unwrap();
                device(dir.path(), "replacement", "firmware", 100, 30);
                Ok(())
            })
            .await
            .is_err()
    );
    let current = backend.read().await.unwrap().unwrap();
    assert_eq!(current.device, "replacement");
    assert_eq!(current.percentage, 30);
}
