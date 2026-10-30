use super::*;
use std::sync::{Arc, atomic::Ordering};

mod activation;
mod support;
use support::{BORROWED, DISTRO_BUS_NAME, Fixture, Services};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn cancelled() -> AtomicBool {
    AtomicBool::new(false)
}

fn assert_reaped(pid: u32) {
    assert!(
        !Path::new("/proc").join(pid.to_string()).exists(),
        "owned child survived cleanup"
    );
}

#[test]
fn shared_capture_is_owned_ready_before_refresh_and_uses_exact_session_configuration() {
    let fixture = Fixture::new();
    let names: Vec<_> = BORROWED
        .iter()
        .copied()
        .chain(std::iter::once(DISTRO_BUS_NAME))
        .collect();
    let mut borrowed = fixture.foreign(&names);
    let mut services = Services::default();
    services
        .0
        .start_plan(
            &fixture.paths,
            fixture.plan(false),
            deadline(),
            &cancelled(),
        )
        .unwrap();
    assert_eq!(services.0.entries.len(), 1);
    let entry = &services.0.entries[0];
    let pid = entry.pid().unwrap();
    assert!(entry.adopted.is_none());
    assert!(
        services
            .0
            .process_json_fields()
            .contains(&format!(",\"portal-wlr\":{pid}"))
    );
    let launch = fixture.snapshot("capture");
    assert_eq!(launch["pid"], pid);
    assert_eq!(
        launch["args"],
        serde_json::json!([
            "-c",
            fixture.paths.session_dir.join("screencast.conf"),
            "--bus-name=org.freedesktop.impl.portal.desktop.hearthspace_wlr",
            "--no-restore"
        ])
    );
    for (key, expected) in &fixture.session.vars {
        if key == "HEARTHSPACE_SESSION_BUS_ADDRESS" {
            assert_eq!(launch["env"]["DBUS_SESSION_BUS_ADDRESS"], expected.as_str());
        } else {
            assert_eq!(launch["env"][key], expected.as_str());
        }
    }
    assert_eq!(launch["env"]["GTK_USE_PORTAL"], "0");
    assert_eq!(launch["env"]["GDK_BACKEND"], "wayland");
    assert_eq!(launch["env"]["XDPW_PERSIST_MODE"], "none");
    let config = launch["config"].as_str().unwrap();
    assert!(config.starts_with("[screencast]\nmax_fps=30\nchooser_type=dmenu\n"));
    let command = config
        .lines()
        .find_map(|line| line.strip_prefix("chooser_cmd="))
        .unwrap();
    let output = Command::new("/bin/sh")
        .args(["-c", command])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim_end(),
        fixture.picker.to_str().unwrap()
    );
    let calls = fixture.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["args"][3], "xdg-desktop-portal-gtk.service");
    assert_eq!(calls[1]["args"][3], "xdg-desktop-portal.service");
    for call in &calls {
        assert_eq!(call["capture_pid"], pid);
    }
    services.0.shutdown();
    assert_reaped(pid);
    assert!(borrowed.running());
    for name in names {
        assert_eq!(
            owner_pid(&fixture.bus, name, deadline(), &cancelled()).unwrap(),
            Some(borrowed.pid())
        );
    }
    assert_eq!(fixture.calls(), calls, "cleanup touched borrowed units");
}

#[test]
fn private_frontend_starts_after_owned_capture_and_all_children_are_reaped() {
    let fixture = Fixture::new();
    let mut services = Services::default();
    services
        .0
        .start_plan(&fixture.paths, fixture.plan(true), deadline(), &cancelled())
        .unwrap();
    let pids: Vec<_> = services
        .0
        .entries
        .iter()
        .map(|entry| {
            assert!(entry.adopted.is_none());
            entry.pid().unwrap()
        })
        .collect();
    assert_eq!(pids.len(), 3);
    assert_eq!(fixture.snapshot("desktop-portal")["capture_pid"], pids[0]);
    assert!(fixture.calls().is_empty());
    services.0.shutdown();
    for pid in pids {
        assert_reaped(pid);
    }
    assert_eq!(
        owner_pid(&fixture.bus, screencast::BUS_NAME, deadline(), &cancelled()).unwrap(),
        None
    );
}

#[test]
fn existing_foreign_capture_owner_is_never_adopted_replaced_or_signalled() {
    let fixture = Fixture::new();
    let mut foreign = fixture.foreign(&[screencast::BUS_NAME]);
    let mut services = Services::default();
    let error = services
        .0
        .start_plan(
            &fixture.paths,
            fixture.plan(false),
            deadline(),
            &cancelled(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("refusing to replace or adopt"));
    assert!(services.0.entries.is_empty());
    assert!(fixture.calls().is_empty());
    assert!(
        !fixture.root.join("capture.json").exists(),
        "backend was spawned despite foreign ownership"
    );
    services.0.shutdown();
    assert!(foreign.running());
    assert_eq!(
        owner_pid(&fixture.bus, screencast::BUS_NAME, deadline(), &cancelled()).unwrap(),
        Some(foreign.pid())
    );
}

#[test]
fn restart_refuses_a_new_foreign_owner_and_exhausts_its_bounded_attempts() {
    let fixture = Fixture::new();
    let mut services = Services::default();
    services
        .0
        .start_plan(&fixture.paths, fixture.plan(true), deadline(), &cancelled())
        .unwrap();
    services.0.entries[0].stop();
    let original = fixture.snapshot("capture");
    let mut foreign = fixture.foreign(&[screencast::BUS_NAME]);
    assert!(services.0.poll(&cancelled()));
    assert_eq!(services.0.entries[0].restarts_left, 1);
    assert!(services.0.entries[0].pid().is_none());
    assert!(services.0.poll(&cancelled()));
    assert_eq!(services.0.entries[0].restarts_left, 0);
    assert!(!services.0.poll(&cancelled()));
    assert_eq!(
        fixture.snapshot("capture"),
        original,
        "restart spawned a competing backend"
    );
    services.0.shutdown();
    assert!(foreign.running());
}

#[test]
fn failed_backend_start_is_reaped_before_any_shared_refresh() {
    let fixture = Fixture::new();
    let mut borrowed = fixture.foreign(BORROWED);
    fs::write(fixture.root.join("exit-capture"), "").unwrap();
    let mut services = Services::default();
    let error = services
        .0
        .start_plan(
            &fixture.paths,
            fixture.plan(false),
            deadline(),
            &cancelled(),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("portal-wlr exited during startup")
    );
    assert_reaped(fixture.snapshot("capture")["pid"].as_u64().unwrap() as u32);
    assert!(services.0.process_json_fields().is_empty());
    assert!(fixture.calls().is_empty());
    assert!(borrowed.running());
}

#[test]
fn failed_shared_refresh_cleans_capture_but_preserves_borrowed_services() {
    let fixture = Fixture::new();
    let mut borrowed = fixture.foreign(BORROWED);
    fs::write(fixture.root.join("fail-manager"), "").unwrap();
    let mut services = Services::default();
    let error = services
        .0
        .start_plan(
            &fixture.paths,
            fixture.plan(false),
            deadline(),
            &cancelled(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("could not restart"));
    assert_reaped(fixture.snapshot("capture")["pid"].as_u64().unwrap() as u32);
    assert!(services.0.process_json_fields().is_empty());
    assert!(borrowed.running());
}

#[test]
fn failed_private_frontend_cleans_previously_ready_capture_and_gtk() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("exit-desktop-portal"), "").unwrap();
    let mut services = Services::default();
    let error = services
        .0
        .start_plan(&fixture.paths, fixture.plan(true), deadline(), &cancelled())
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("desktop-portal exited during startup")
    );
    for label in ["capture", "portal-gtk", "desktop-portal"] {
        assert_reaped(fixture.snapshot(label)["pid"].as_u64().unwrap() as u32);
    }
    assert!(services.0.entries.is_empty());
}

#[test]
fn cancellation_during_backend_readiness_reaps_the_child_without_refresh() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("stall-capture"), "").unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let ready = fixture.root.join("capture.json");
    let worker = thread::spawn(move || {
        let limit = deadline();
        while !ready.exists() && Instant::now() < limit {
            thread::sleep(Duration::from_millis(5));
        }
        signal.store(true, Ordering::SeqCst);
    });
    let mut services = Services::default();
    let error = services
        .0
        .start_plan(&fixture.paths, fixture.plan(false), deadline(), &cancelled)
        .unwrap_err();
    worker.join().unwrap();
    assert_eq!(
        error.downcast_ref::<io::Error>().unwrap().kind(),
        io::ErrorKind::Interrupted
    );
    assert_reaped(fixture.snapshot("capture")["pid"].as_u64().unwrap() as u32);
    assert!(fixture.calls().is_empty());
}

#[test]
fn failed_restart_readiness_preserves_the_second_capture_restart() {
    let fixture = Fixture::new();
    let mut services = Services::default();
    services
        .0
        .start_plan(&fixture.paths, fixture.plan(true), deadline(), &cancelled())
        .unwrap();
    services.0.entries[0].stop();
    fs::write(fixture.root.join("exit-capture"), "").unwrap();
    assert!(services.0.poll(&cancelled()));
    assert_eq!(services.0.entries[0].restarts_left, 1);
    assert!(services.0.entries[0].pid().is_none());
    assert_reaped(fixture.snapshot("capture")["pid"].as_u64().unwrap() as u32);
    fs::remove_file(fixture.root.join("exit-capture")).unwrap();
    assert!(services.0.poll(&cancelled()));
    let pid = services.0.entries[0].pid().unwrap();
    assert_eq!(
        owner_pid(&fixture.bus, screencast::BUS_NAME, deadline(), &cancelled()).unwrap(),
        Some(pid)
    );
    assert_eq!(services.0.entries[0].restarts_left, 0);
    assert!(!services.0.poll(&cancelled()));
}
