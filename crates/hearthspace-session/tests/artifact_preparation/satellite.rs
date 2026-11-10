//! The mandatory X11 helper must finish preparation before native ownership.
use super::*;

#[test]
fn failed_satellite_build_prevents_guard_preparation_and_owner_submission() {
    let mut fixture = Fixture::new();
    fixture.satellite_mode = "fail";
    fixture.start(true, "fresh");
    assert!(!fixture.wait().success());
    assert!(
        fixture
            .output()
            .contains("hearthspace-satellite build failed"),
        "{}",
        fixture.output()
    );
    assert!(fixture.root.join("locker-build-called").exists());
    assert!(fixture.root.join("satellite-build-called").exists());
    assert!(!fixture.root.join("build-called").exists());
    assert!(!fixture.root.join("owner-submitted").exists());
    assert!(!fixture.root.join("session").exists());
}

#[test]
fn stalled_satellite_build_cannot_outlive_the_shared_startup_deadline() {
    let mut fixture = Fixture::new();
    fixture.satellite_mode = "hang";
    fixture.start(true, "fresh");
    assert!(!fixture.wait().success());
    assert!(
        fixture.output().contains("deadline"),
        "{}",
        fixture.output()
    );
    let pid = fs::read_to_string(fixture.root.join("satellite-build-pid")).unwrap();
    assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
    assert!(!fixture.root.join("build-called").exists());
    assert!(!fixture.root.join("owner-submitted").exists());
    assert!(!fixture.root.join("session").exists());
}
