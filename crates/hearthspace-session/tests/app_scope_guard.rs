//! Process-only regression: no real user manager, desktop or GPU is needed.
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

struct Fixture {
    runtime: std::path::PathBuf,
    parent: Child,
    guard: Child,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let runtime = std::env::temp_dir().join(format!(
            "hearthspace-app-guard-{}-{name}",
            std::process::id()
        ));
        fs::DirBuilder::new().mode(0o700).create(&runtime).unwrap();
        let parent = Command::new("/usr/bin/sleep").arg("30").spawn().unwrap();
        let stat = fs::read_to_string(format!("/proc/{}/stat", parent.id())).unwrap();
        let start = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap();
        let guard = Command::new(env!("CARGO_BIN_EXE_hearthspace-app-scope-guard"))
            .arg(parent.id().to_string())
            .arg(start)
            .arg(&runtime)
            .spawn()
            .unwrap();
        let mut fixture = Self {
            runtime,
            parent,
            guard,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while !fixture.runtime.join("app-scope-guard-ready").is_file() {
            assert!(
                fixture.guard.try_wait().unwrap().is_none(),
                "guard exited before readiness"
            );
            assert!(Instant::now() < deadline, "guard failed readiness");
            thread::sleep(Duration::from_millis(10));
        }
        fixture
    }

    fn assert_guard_exits(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = self.guard.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "guard did not end with its session"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.parent.kill();
        let _ = self.parent.wait();
        let _ = self.guard.kill();
        let _ = self.guard.wait();
        let _ = fs::remove_dir_all(&self.runtime);
    }
}

#[test]
fn guard_exits_after_supervisor_death() {
    let mut fixture = Fixture::new("death");
    assert!(fixture.guard.try_wait().unwrap().is_none());
    fixture.parent.kill().unwrap();
    // An unreaped zombie must not keep the user's app scopes alive either.
    fixture.assert_guard_exits();
    fixture.parent.wait().unwrap();
}

#[test]
fn closing_marker_ends_live_supervisor_scopes() {
    let mut fixture = Fixture::new("closing");
    assert!(fixture.guard.try_wait().unwrap().is_none());
    fs::write(fixture.runtime.join("app-scopes-closing"), "").unwrap();
    fixture.assert_guard_exits();
    assert!(fixture.parent.try_wait().unwrap().is_none());
}
