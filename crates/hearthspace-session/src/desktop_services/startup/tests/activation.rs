use super::*;

// A regular method call with D-Bus auto-activation exercises routing at
// the broker boundary. The backend fixture answers it; no frontend is emulated.
fn request(fixture: &Fixture, name: &str) -> std::process::Output {
    bounded_command::output(
        Command::new("busctl").args([
            "--address",
            &fixture.bus,
            "--timeout=1",
            "--auto-start=yes",
            "call",
            name,
            "/org/freedesktop/portal/desktop",
            "org.hearthspace.TestCapture",
            "Ready",
        ]),
        Instant::now() + Duration::from_secs(3),
        &cancelled(),
    )
    .unwrap()
}

#[test]
fn crash_gap_cannot_activate_the_distro_backend_and_restart_keeps_the_chooser() {
    let fixture = Fixture::new();
    let mut borrowed = fixture.foreign(BORROWED);
    prepare(&fixture.paths, false).unwrap();
    let descriptor = fs::read_to_string(
        fixture
            .paths
            .session_dir
            .join("share/xdg-desktop-portal/portals/wlr.portal"),
    )
    .unwrap();
    let destination = descriptor
        .lines()
        .find_map(|line| line.strip_prefix("DBusName="))
        .unwrap();
    assert_eq!(
        destination,
        "org.freedesktop.impl.portal.desktop.hearthspace_wlr"
    );
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
    let original = fixture.snapshot("capture");
    let pid = services.0.entries[0].pid().unwrap();
    assert!(request(&fixture, destination).status.success());

    services.0.entries[0].stop();
    assert_reaped(pid);
    assert!(!request(&fixture, destination).status.success());
    assert!(
        !fixture.root.join("distro-activated").exists(),
        "the crash-gap request activated a distro backend"
    );
    assert_eq!(
        owner_pid(&fixture.bus, DISTRO_BUS_NAME, deadline(), &cancelled()).unwrap(),
        None
    );

    assert!(services.0.poll(&cancelled()));
    let restarted = fixture.snapshot("capture");
    assert_eq!(restarted["args"], original["args"]);
    assert_eq!(restarted["config"], original["config"]);
    assert_eq!(restarted["env"], original["env"]);
    let restarted_pid = services.0.entries[0].pid().unwrap();
    assert_eq!(
        owner_pid(&fixture.bus, destination, deadline(), &cancelled()).unwrap(),
        Some(restarted_pid)
    );
    let response = request(&fixture, destination);
    assert!(response.status.success());
    assert_eq!(String::from_utf8(response.stdout).unwrap().trim(), "b true");
    assert!(!fixture.root.join("distro-activated").exists());

    // Positive control: the distro name really is activatable on this broker.
    // Its sentinel exits immediately, so it cannot leave an unowned service.
    assert!(!request(&fixture, DISTRO_BUS_NAME).status.success());
    assert_eq!(
        fs::read_to_string(fixture.root.join("distro-activated")).unwrap(),
        "activated\n"
    );
    services.0.shutdown();
    assert_reaped(restarted_pid);
    assert!(borrowed.running());
}

#[test]
fn an_unpatched_backend_rejects_required_options_before_private_frontend_startup() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("unpatched-backend"), "").unwrap();
    let mut services = Services::default();
    let error = services
        .0
        .start_plan(&fixture.paths, fixture.plan(true), deadline(), &cancelled())
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("portal-wlr exited during startup")
    );
    assert_reaped(fixture.snapshot("capture")["pid"].as_u64().unwrap() as u32);
    assert!(services.0.entries.is_empty());
    for label in ["portal-gtk", "desktop-portal"] {
        assert!(!fixture.root.join(format!("{label}.json")).exists());
    }
    assert!(fixture.calls().is_empty());
}
