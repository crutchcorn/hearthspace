use super::*;
use crate::desktop_services::DesktopServices;
use std::{
    fs,
    sync::{Arc, atomic::Ordering},
};

mod support;
use support::Fixture;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn missing_data_services_start_before_ordered_display_refresh() {
    let mut fixture = Fixture::new();
    let mut services = DesktopServices::default();
    let cancelled = AtomicBool::new(false);
    fixture
        .start(&mut services, deadline(), &cancelled)
        .unwrap();

    let calls = fixture.calls();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls.iter().map(|call| &call["args"]).collect::<Vec<_>>(),
        vec![
            &serde_json::json!([
                "--user",
                "--no-pager",
                "start",
                "xdg-permission-store.service"
            ]),
            &serde_json::json!([
                "--user",
                "--no-pager",
                "start",
                "xdg-document-portal.service"
            ]),
            &serde_json::json!([
                "--user",
                "--no-pager",
                "restart",
                "xdg-desktop-portal-gtk.service"
            ]),
            &serde_json::json!([
                "--user",
                "--no-pager",
                "restart",
                "xdg-desktop-portal.service"
            ]),
        ]
    );
    for call in &calls {
        assert_eq!(
            call["runtime"],
            fixture.runtime.canonicalize().unwrap().to_str().unwrap()
        );
        assert_eq!(call["bus"], fixture.bus);
    }
    for &(name, _, bus_name) in DATA_SERVICES.iter().chain(DISPLAY_SERVICES) {
        assert!(
            services
                .process_json_fields()
                .contains(&format!(",\"{name}\":{}", fixture.pid()))
        );
        assert_eq!(
            owner_pid(&fixture.bus, bus_name, deadline(), &cancelled).unwrap(),
            Some(fixture.pid())
        );
    }
    assert!(services.entries.is_empty(), "shared services were adopted");
    assert!(!services.poll(&cancelled));
    services.shutdown();
    fixture.assert_running();
    assert_eq!(fixture.calls(), calls, "cleanup managed shared user units");
    assert!(services.process_json_fields().is_empty());
}

#[test]
fn active_data_owners_are_borrowed_and_survive_display_startup_failure() {
    let mut fixture = Fixture::new();
    fixture.publish(DATA_SERVICES[0].1, DATA_SERVICES[0].2);
    fixture.publish(DATA_SERVICES[1].1, DATA_SERVICES[1].2);
    fs::write(fixture.root().join("fail"), "xdg-desktop-portal.service").unwrap();
    let mut services = DesktopServices::default();
    let cancelled = AtomicBool::new(false);
    let error = fixture
        .start(&mut services, deadline(), &cancelled)
        .unwrap_err();
    assert!(error.to_string().contains("xdg-desktop-portal.service"));
    assert_eq!(services.shared.entries.len(), 3);
    let calls = fixture.calls();
    assert_eq!(calls.len(), 2, "active data services must not be restarted");
    assert_eq!(calls[0]["args"][3], "xdg-desktop-portal-gtk.service");
    assert_eq!(calls[1]["args"][3], "xdg-desktop-portal.service");

    assert!(!services.poll(&cancelled));
    services.shutdown();
    fixture.assert_running();
    for &(_, _, bus_name) in DATA_SERVICES {
        assert_eq!(
            owner_pid(&fixture.bus, bus_name, deadline(), &cancelled).unwrap(),
            Some(fixture.pid()),
            "startup cleanup killed a borrowed data service"
        );
    }
    assert_eq!(fixture.calls(), calls);
}

#[test]
fn shared_service_exit_does_not_trigger_session_restart_or_stop() {
    let mut fixture = Fixture::new();
    let mut services = DesktopServices::default();
    let cancelled = AtomicBool::new(false);
    fixture
        .start(&mut services, deadline(), &cancelled)
        .unwrap();
    let calls = fixture.calls();
    fixture.stop_helper();
    assert!(!services.poll(&cancelled));
    services.shutdown();
    assert_eq!(fixture.calls(), calls);
}

#[test]
fn cancellation_interrupts_a_stalled_manager_and_preserves_borrowed_owners() {
    let mut fixture = Fixture::new();
    fixture.publish(DATA_SERVICES[0].1, DATA_SERVICES[0].2);
    fixture.publish(DATA_SERVICES[1].1, DATA_SERVICES[1].2);
    fs::write(fixture.root().join("stall"), "").unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancellation = cancelled.clone();
    let root = fixture.root().to_path_buf();
    let worker = thread::spawn(move || {
        let deadline = deadline();
        while manager_pid(&root).is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        cancellation.store(true, Ordering::SeqCst);
    });
    let mut services = DesktopServices::default();
    let error = fixture
        .start(&mut services, deadline(), &cancelled)
        .unwrap_err();
    worker.join().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_manager_reaped(&fixture);
    services.shutdown();
    fixture.assert_running();
    assert_eq!(
        fixture.calls().len(),
        1,
        "startup continued after cancellation"
    );
}

#[test]
fn stalled_manager_expires_without_stopping_borrowed_services() {
    let mut fixture = Fixture::new();
    fixture.publish(DATA_SERVICES[0].1, DATA_SERVICES[0].2);
    fixture.publish(DATA_SERVICES[1].1, DATA_SERVICES[1].2);
    fs::write(fixture.root().join("stall"), "").unwrap();
    let mut services = DesktopServices::default();
    let started = Instant::now();
    let error = fixture
        .start(
            &mut services,
            started + Duration::from_secs(3),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_manager_reaped(&fixture);
    services.shutdown();
    fixture.assert_running();
    assert_eq!(fixture.calls().len(), 1);
}

#[test]
fn successful_manager_without_a_bus_owner_does_not_pass_readiness() {
    let fixture = Fixture::new();
    fs::write(fixture.root().join("no-publication"), "").unwrap();
    let mut services = DesktopServices::default();
    let error = fixture
        .start(
            &mut services,
            Instant::now() + Duration::from_secs(2),
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(fixture.calls().len(), 1);
    assert!(services.process_json_fields().is_empty());
}

fn assert_manager_reaped(fixture: &Fixture) {
    let pid = manager_pid(fixture.root()).expect("manager never published a complete PID");
    assert!(
        !Path::new("/proc").join(pid.to_string()).exists(),
        "manager helper survived"
    );
}

fn manager_pid(root: &Path) -> Option<u32> {
    fs::read_to_string(root.join("stalled-pid"))
        .ok()?
        .strip_suffix('\n')?
        .parse()
        .ok()
}
