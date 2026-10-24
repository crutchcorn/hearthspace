//! An actual managed child of a nested supervisor cannot claim its parent bus.
use super::*;
use crate::process::{
    ManagedProcess, ProcessSpec, poll_process, wait_then_kill_process_with_timeout,
};
use clap::Parser;
use hearthspace_desktop::{
    activation::MARKER,
    environment::{APP_DISPLAY, APP_RUNTIME},
};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MODE: &str = "HEARTHSPACE_NESTED_ENV_PROBE";
const TEST: &str =
    "session_env::tests::nested::nested_child_drops_parent_owner_and_explicit_generation_wins";

#[test]
fn nested_child_drops_parent_owner_and_explicit_generation_wins() {
    match env::var(MODE).as_deref() {
        Ok("child") => {
            assert!(env::var_os(MARKER).is_none());
            assert!(env::var_os("HEARTHSPACE_SESSION_BUS_ADDRESS").is_none());
            assert_eq!(
                env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
                "unix:path=/synthetic/borrowed-bus"
            );
            // The real activation helper must reject this borrowed bus before
            // any GIO activation. The existing direct launch policy separately
            // excludes borrowed routing from its isolated application children.
            assert!(
                hearthspace_desktop::file_launch::run_native_gio_helper(std::iter::empty())
                    .unwrap_err()
                    .contains("selected desktop session bus is unavailable")
            );
            verify_direct_routing(None);
            assert_eq!(
                env::var(APP_RUNTIME).unwrap(),
                env::var("XDG_RUNTIME_DIR").unwrap()
            );
            assert_eq!(env::var(APP_DISPLAY).unwrap(), "nested-test");
            return;
        }
        Ok("owned-child") => {
            assert_eq!(env::var(MARKER).unwrap(), "new-explicit-generation");
            assert_eq!(
                env::var("HEARTHSPACE_SESSION_BUS_ADDRESS").unwrap(),
                "unix:path=/synthetic/current-bus"
            );
            verify_direct_routing(Some("unix:path=/synthetic/current-bus"));
            return;
        }
        Ok("direct-child") => {
            let expected = env::var("EXPECTED_BUS").unwrap();
            assert_eq!(
                env::var("DBUS_SESSION_BUS_ADDRESS").ok().as_deref(),
                (!expected.is_empty()).then_some(expected.as_str())
            );
            assert_eq!(env::var("WAYLAND_DISPLAY").unwrap(), "nested-test");
            return;
        }
        Ok("supervisor") => {
            run_nested();
            return;
        }
        _ => {}
    }
    let mut child = Command::new(env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(MODE, "supervisor")
        .env(MARKER, "parent-generation")
        .env(
            "HEARTHSPACE_SESSION_BUS_ADDRESS",
            "unix:path=/synthetic/parent-owned-bus",
        )
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/synthetic/borrowed-bus",
        )
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("nested environment regression timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn run_nested() {
    assert_eq!(env::var(MARKER).unwrap(), "parent-generation");
    let root = env::temp_dir().join(format!("hearthspace-nested-env-{}", process::id()));
    fs::create_dir(&root).unwrap();
    let session_dir = root.join("nested");
    prepare_session_directory(&session_dir, false).unwrap();
    let paths = SessionPaths {
        host_runtime_dir: root.clone(),
        applications_dir: session_dir.join("share/applications"),
        settings_socket: session_dir.join("settings.sock"),
        session_socket: session_dir.join("session.sock"),
        shell_command_socket: session_dir.join("command.sock"),
        session_dir,
    };
    let args = Args::parse_from([
        "session",
        "--installed",
        "--backend",
        "nested",
        "--wayland-display",
        "nested-test",
    ]);
    let session = create_session_env(&args, RunMode::InstalledBinaries, &paths).unwrap();
    assert!(!session.vars.iter().any(|(key, _)| key == MARKER));
    run_child(session.vars.clone(), "child");
    let mut owned = session.vars;
    owned.push((MARKER.into(), "new-explicit-generation".into()));
    owned.push((
        "HEARTHSPACE_SESSION_BUS_ADDRESS".into(),
        "unix:path=/synthetic/current-bus".into(),
    ));
    run_child(owned, "owned-child");
    fs::remove_dir_all(root).unwrap();
}

fn run_child(mut envs: Vec<(String, String)>, mode: &str) {
    envs.push((MODE.into(), mode.into()));
    let mut child = Some(
        ManagedProcess::spawn(
            ProcessSpec {
                name: "nested-env-probe",
                program: env::current_exe().unwrap().to_str().unwrap().into(),
                args: vec!["--exact".into(), TEST.into(), "--nocapture".into()],
                envs,
                env_removals: Vec::new(),
                current_dir: None,
            },
            0,
        )
        .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if let Some(status) = poll_process(&mut child).unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    wait_then_kill_process_with_timeout(&mut child, Duration::from_millis(10));
    assert!(
        status.is_some_and(|status| status.success()),
        "nested child failed"
    );
}

fn verify_direct_routing(expected: Option<&str>) {
    let argv = vec![
        env::current_exe().unwrap().to_str().unwrap().into(),
        "--exact".into(),
        TEST.into(),
        "--nocapture".into(),
    ];
    let mut direct = hearthspace_desktop::app_catalog::launch_command(
        &argv,
        "nested-test",
        &[
            (MODE, "direct-child"),
            ("EXPECTED_BUS", expected.unwrap_or("")),
        ],
        None,
    )
    .unwrap();
    assert!(direct.status().unwrap().success());
}
