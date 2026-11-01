//! Dispatch real shortcuts with an isolated, observable terminal executable.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use calloop::EventLoop;
use hearthspace_ipc::command::ShellCommand;
use wayland_server::Display;

use crate::compositor::{
    App, frame_callbacks::test_support::app::new_app, shell_integration::ControlReply,
};

const CHILD: &str =
    "compositor::shell_integration::launch::startup_tests::shortcut_in_isolated_environment";
const BLOCKED: &str = "applications cannot launch until compositor startup completes";

#[test]
fn startup_blocks_terminal_shortcuts_and_later_wayland_launches_still_work() {
    let root = tempfile::tempdir().unwrap();
    for directory in ["home", "data", "config", "runtime", "bin"] {
        fs::create_dir_all(root.path().join(directory)).unwrap();
    }
    let foot = root.path().join("bin/foot");
    fs::write(
        &foot,
        "#!/bin/sh\nprintf 'launched\\n' >> \"$HEARTHSPACE_TEST_TERMINAL_MARKER\"\n",
    )
    .unwrap();
    fs::set_permissions(foot, fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env_clear()
        .env(
            "HEARTHSPACE_TEST_TERMINAL_MARKER",
            root.path().join("launches"),
        )
        .env("HOME", root.path().join("home"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .env("XDG_DATA_DIRS", root.path().join("empty"))
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_CONFIG_DIRS", root.path().join("empty"))
        .env("XDG_RUNTIME_DIR", root.path().join("runtime"))
        .env("PATH", root.path().join("bin"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn terminal_shortcut(app: &mut App) {
    for command in [
        ShellCommand::KeyDown(125),
        ShellCommand::KeyDown(28),
        ShellCommand::KeyUp(28),
        ShellCommand::KeyUp(125),
    ] {
        assert!(matches!(app.run_control_action(command), ControlReply::Ok));
    }
}

#[test]
fn shortcut_in_isolated_environment() {
    let Some(marker) = std::env::var_os("HEARTHSPACE_TEST_TERMINAL_MARKER").map(PathBuf::from)
    else {
        return;
    };
    let display: Display<App> = Display::new().unwrap();
    let event_loop = EventLoop::try_new().unwrap();
    let mut app = new_app(&display.handle(), event_loop.handle());
    app.application_launch_ready = false;
    app.next_spawn_output_name = Some("preserve-pending-output".into());

    assert_eq!(app.spawn_foot().unwrap_err(), BLOCKED);
    assert!(
        matches!(app.launch_app_reply("missing.desktop", None), ControlReply::Err(error) if error == BLOCKED)
    );
    assert!(matches!(app.launch_files("{}"), ControlReply::Err(error) if error == BLOCKED));
    terminal_shortcut(&mut app);
    assert_eq!(
        app.next_spawn_output_name.as_deref(),
        Some("preserve-pending-output")
    );
    assert!(
        !marker.exists(),
        "startup shortcut must not start a terminal"
    );

    // This gate belongs to completed startup. Losing Xwayland later must not
    // disable launching Wayland clients (or require Xwayland in explicit mode).
    app.application_launch_ready = true;
    #[cfg(feature = "xwayland")]
    {
        app.xwayland_required = true;
        app.xdisplay = None;
        app.xwm = None;
        assert!(!app.xwayland_ready());
    }
    terminal_shortcut(&mut app);
    let deadline = Instant::now() + Duration::from_secs(2);
    while fs::read_to_string(&marker).unwrap_or_default().is_empty() {
        assert!(
            Instant::now() < deadline,
            "post-startup terminal did not launch"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fs::read_to_string(marker).unwrap(), "launched\n");
    assert!(app.next_spawn_output_name.is_none());
}
