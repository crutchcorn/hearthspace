use super::*;

fn activation_only_fixture() -> Fixture {
    let fixture = fixture();
    fs::write(
        fixture
            .path()
            .join(format!("share/applications/{APP}.desktop")),
        "[Desktop Entry]\nType=Application\nName=Cold Player\nDBusActivatable=true\n",
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
