//! Masonry actions can retire windows with presentation work already queued.
use std::{env, fs, os::unix::process::CommandExt};

use smithay::reexports::rustix::process::{Pid, Signal, kill_process_group};

use super::navigation::wait_windows;
use super::*;

#[path = "masonry_window_lifecycle/client.rs"]
mod client;

const CASE_ENV: &str = "HEARTHSPACE_MASONRY_LIFECYCLE_CASE";
const CHILD_TEST: &str = "masonry_window_lifecycle::masonry_lifecycle_client";
const SURVIVOR: &str = "Masonry lifecycle survivor";

struct OwnedClient(Child);

fn action(compositor: &HeadlessCompositor, command: impl AsRef<str>) {
    let mut stream = UnixStream::connect(&compositor.command_socket_path).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .write_all(format!("{}\n", command.as_ref()).as_bytes())
        .unwrap();
    assert_eq!(read_line(&mut stream), "ok\n", "{}", command.as_ref());
}

impl Drop for OwnedClient {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = kill_process_group(Pid::from_raw(self.0.id() as i32).unwrap(), Signal::KILL);
            let _ = self.0.wait();
        }
    }
}

fn exercise(case: &str) {
    let _guard = headless_test_guard();
    let mut compositor =
        HeadlessCompositor::spawn_with_command("800x600", None, None, 1, false, |command| {
            command
                .env("LIBGL_ALWAYS_SOFTWARE", "1")
                .env("VK_DRIVER_FILES", "/usr/share/vulkan/icd.d/lvp_icd.json")
                .env_remove("DBUS_SESSION_BUS_ADDRESS")
                .env_remove("DBUS_SYSTEM_BUS_ADDRESS")
                .env_remove("AT_SPI_BUS_ADDRESS")
                .env_remove("HEARTHSPACE_SETTINGS_SOCKET")
                .env_remove("HEARTHSPACE_LOG_FILE");
        });
    drop(compositor.wait_for_socket());
    let directory = compositor.runtime_dir.path();
    let log_path = directory.join("masonry-lifecycle.log");
    let log = fs::File::create(&log_path).unwrap();
    let mut client = OwnedClient(
        Command::new(env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .env_clear()
            .env(CASE_ENV, case)
            .env("XDG_RUNTIME_DIR", directory)
            .env(
                "WAYLAND_DISPLAY",
                format!("hearthspace-test-{}", std::process::id()),
            )
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .env("VK_DRIVER_FILES", "/usr/share/vulkan/icd.d/lvp_icd.json")
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !directory.join("retired-ready").exists() {
        assert!(
            client.0.try_wait().unwrap().is_none() && Instant::now() < deadline,
            "{case}: client failed before deferred work drained:\n{}",
            fs::read_to_string(&log_path).unwrap()
        );
        thread::sleep(Duration::from_millis(20));
    }
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 1 && windows[0]["title"] == SURVIVOR
    });
    action(
        &compositor,
        format!("resize-window {} 360 200", windows[0]["id"]),
    );
    while !directory.join("survivor-rendered").exists() {
        assert!(
            client.0.try_wait().unwrap().is_none() && Instant::now() < deadline,
            "{case}: survivor did not render after a compositor resize:\n{}",
            fs::read_to_string(&log_path).unwrap()
        );
        thread::sleep(Duration::from_millis(20));
    }
    let windows = wait_windows(&compositor, |windows| {
        windows.len() == 1
            && windows[0]["title"] == SURVIVOR
            && windows[0]["content_width"] == 360
            && windows[0]["content_height"] == 200
    });
    let window = &windows[0];
    assert_eq!(window["titlebar_height"], 0);
    action(&compositor, format!("focus-window {}", window["id"]));
    action(
        &compositor,
        format!(
            "pointer-motion-abs {} {}",
            window["x"].as_f64().unwrap() + window["width"].as_f64().unwrap() / 2.0,
            window["y"].as_f64().unwrap() + window["height"].as_f64().unwrap() / 2.0
        ),
    );
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    let status = loop {
        if let Some(status) = client.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "{case}: survivor did not accept input:\n{}",
            fs::read_to_string(&log_path).unwrap()
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "{}",
        fs::read_to_string(&log_path).unwrap()
    );
    assert!(directory.join("survivor-clicked").exists());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn masonry_redraw_then_self_close_keeps_survivor_usable() {
    exercise("redraw-self");
}

#[test]
fn masonry_animation_then_other_window_close_keeps_survivor_usable() {
    exercise("animation-other");
}

#[test]
fn masonry_retired_first_frame_is_never_presented() {
    exercise("first-frame");
}

#[test]
fn masonry_lifecycle_client() {
    // The isolated child owns Winit's one event loop and its display environment.
    if let Ok(case) = env::var(CASE_ENV) {
        client::run(case);
    }
}
