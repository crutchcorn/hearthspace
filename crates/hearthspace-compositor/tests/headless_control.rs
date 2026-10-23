use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

#[path = "support/serialization.rs"]
mod serialization;
#[path = "support/shell_window_lifecycle.rs"]
mod shell_window_lifecycle;
#[path = "support/startup.rs"]
mod startup;

#[cfg(feature = "test-apps")]
#[path = "support/accessibility.rs"]
mod accessibility_diagnostics;
#[cfg(feature = "test-apps")]
#[path = "support/canvas_map.rs"]
mod canvas_map;
#[cfg(feature = "test-apps")]
#[path = "support/canvas_map_client.rs"]
mod canvas_map_client;
#[cfg(feature = "test-apps")]
#[path = "support/canvas_peek.rs"]
mod canvas_peek;
#[cfg(feature = "test-apps")]
#[path = "support/canvas_search.rs"]
mod canvas_search;
#[cfg(feature = "test-apps")]
#[path = "support/canvas_zoom.rs"]
mod canvas_zoom;
#[cfg(feature = "test-apps")]
#[path = "support/desktop_context.rs"]
mod desktop_context;
#[cfg(feature = "test-apps")]
#[path = "support/desktop_files.rs"]
mod desktop_files;
#[cfg(feature = "test-apps")]
#[path = "support/device_controls.rs"]
mod device_controls;
#[cfg(feature = "test-apps")]
#[path = "support/frame_layout.rs"]
mod frame_layout;
#[cfg(feature = "test-apps")]
#[path = "support/grid_snapping.rs"]
mod grid_snapping;
#[cfg(feature = "test-apps")]
#[path = "support/interactive.rs"]
mod interactive;
#[cfg(feature = "test-apps")]
#[path = "support/layout.rs"]
mod layout;
#[cfg(feature = "test-apps")]
#[path = "support/layout_confirmation.rs"]
mod layout_confirmation;
#[cfg(feature = "test-apps")]
#[path = "support/layout_emphasis.rs"]
mod layout_emphasis;
#[cfg(feature = "test-apps")]
#[path = "support/layout_overflow.rs"]
mod layout_overflow;
#[cfg(feature = "test-apps")]
#[path = "support/navigation.rs"]
mod navigation;
#[cfg(feature = "test-apps")]
#[path = "support/overview.rs"]
mod overview;
#[cfg(feature = "test-apps")]
#[path = "support/remote_input.rs"]
mod remote_input;
#[cfg(feature = "test-apps")]
#[path = "support/search_replay.rs"]
mod search_replay;
#[cfg(feature = "test-apps")]
#[path = "support/session_bus.rs"]
mod session_bus;
#[cfg(feature = "test-apps")]
#[path = "support/session_lock.rs"]
mod session_lock;
#[cfg(feature = "test-apps")]
#[path = "support/shell_blur.rs"]
mod shell_blur;
#[cfg(feature = "test-apps")]
#[path = "support/shell_corners.rs"]
mod shell_corners;
#[cfg(feature = "test-apps")]
#[path = "support/shell_decorations.rs"]
mod shell_decorations;
#[cfg(feature = "test-apps")]
#[path = "support/shell_events.rs"]
mod shell_events;
#[cfg(feature = "test-apps")]
#[path = "support/shell_panels.rs"]
mod shell_panels;
#[cfg(feature = "test-apps")]
#[path = "support/titlebar_controls.rs"]
mod titlebar_controls;
#[path = "support/window_bounds.rs"]
mod window_bounds;
#[cfg(feature = "test-apps")]
#[path = "support/window_group.rs"]
mod window_group;
#[cfg(feature = "test-apps")]
#[path = "support/window_placement.rs"]
mod window_placement;
#[cfg(feature = "test-apps")]
#[path = "support/window_states.rs"]
mod window_states;
#[cfg(feature = "test-apps")]
#[path = "support/window_switcher.rs"]
mod window_switcher;
#[cfg(all(feature = "test-apps", feature = "xwayland"))]
#[path = "support/x11_popups.rs"]
mod x11_popups;
#[cfg(all(feature = "test-apps", feature = "xwayland"))]
#[path = "support/xsettings.rs"]
mod xsettings;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
#[cfg(feature = "test-apps")]
const CLIENT_WIDTH: u32 = 800;
#[cfg(feature = "test-apps")]
const CLIENT_HEIGHT: u32 = 600;

static HEADLESS_TEST_LOCK: Mutex<()> = Mutex::new(());

fn headless_test_guard() -> std::sync::MutexGuard<'static, ()> {
    serialization::lock_after_cleanup(&HEADLESS_TEST_LOCK)
}

struct HeadlessCompositor {
    child: Child,
    runtime_dir: tempfile::TempDir,
    command_socket_path: PathBuf,
}

impl HeadlessCompositor {
    fn spawn() -> Self {
        Self::spawn_with_size("320x240")
    }

    fn spawn_with_size(size: &str) -> Self {
        Self::spawn_with_layout(size, None)
    }

    fn spawn_with_layout(size: &str, layout: Option<&Path>) -> Self {
        Self::spawn_with_layout_and_path(size, layout, None)
    }

    fn spawn_with_layout_and_path(size: &str, layout: Option<&Path>, path: Option<&Path>) -> Self {
        Self::spawn_with_options(size, layout, path, 1)
    }

    fn spawn_with_options(
        size: &str,
        layout: Option<&Path>,
        path: Option<&Path>,
        scale: i32,
    ) -> Self {
        Self::spawn_with_remote_options(size, layout, path, scale, false)
    }

    fn spawn_with_remote_options(
        size: &str,
        layout: Option<&Path>,
        path: Option<&Path>,
        scale: i32,
        remote_desktop: bool,
    ) -> Self {
        Self::spawn_with_command(size, layout, path, scale, remote_desktop, |_| {})
    }

    fn spawn_with_command(
        size: &str,
        layout: Option<&Path>,
        path: Option<&Path>,
        scale: i32,
        remote_desktop: bool,
        configure: impl FnOnce(&mut Command),
    ) -> Self {
        let runtime_dir = tempfile::tempdir().expect("create headless runtime dir");
        let command_socket_path = runtime_dir.path().join("command.sock");
        let wayland_display = format!("hearthspace-test-{}", std::process::id());
        let log = std::fs::File::create(runtime_dir.path().join("compositor.log"))
            .expect("create compositor log");
        let mut command = Command::new(env!("CARGO_BIN_EXE_hearthspace"));
        command
            .args(["--headless", "--no-shell", "--headless-size", size])
            .arg(format!("--headless-scale={scale}"))
            // Model operations use a process-independent cache lock. A mock
            // decision server must never contend with the user's live session.
            .env("XDG_CACHE_HOME", runtime_dir.path().join("cache"))
            .env_remove("HEARTHSPACE_LAYOUT_PATH");
        if remote_desktop {
            command.arg("--remote-desktop");
        }
        if let Some(path) = layout {
            command.env("HEARTHSPACE_LAYOUT_PATH", path);
        }
        if let Some(path) = path {
            command.env("PATH", path);
        }
        configure(&mut command);
        let child = command
            .env("XDG_RUNTIME_DIR", runtime_dir.path())
            .env("HEARTHSPACE_COMMAND_SOCKET", &command_socket_path)
            .env("HEARTHSPACE_WAYLAND_DISPLAY", wayland_display)
            .stdin(Stdio::null())
            .stdout(log.try_clone().expect("clone compositor log"))
            .stderr(log)
            .spawn()
            .expect("spawn headless Hearthspace");

        Self {
            child,
            runtime_dir,
            command_socket_path,
        }
    }

    fn wait_for_socket(&mut self) -> UnixStream {
        let stream = startup::wait_for_socket(&mut self.child, &self.command_socket_path)
            .unwrap_or_else(|error| {
                panic!(
                    "headless startup failed for {}: {error}",
                    self.command_socket_path.display()
                )
            });
        // The control listener is created before Wayland. Real protocol clients
        // must not race the later Wayland socket creation after this returns.
        let wayland_socket = self
            .runtime_dir
            .path()
            .join(format!("hearthspace-test-{}", std::process::id()));
        drop(
            startup::wait_for_socket(&mut self.child, &wayland_socket)
                .expect("headless Wayland socket readiness"),
        );
        stream
    }

    fn send_text_command(&self, command: &str) -> String {
        send_text_command(&self.command_socket_path, command)
    }

    fn take_screenshot(&self) -> Vec<u8> {
        take_screenshot(&self.command_socket_path)
    }
}

impl Drop for HeadlessCompositor {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if thread::panicking() {
            self.runtime_dir.disable_cleanup(true);
            eprintln!(
                "headless failure artifacts: {}",
                self.runtime_dir.path().display()
            );
            if let Ok(log) = std::fs::read_to_string(self.runtime_dir.path().join("compositor.log"))
            {
                let mut tail: Vec<_> = log.lines().rev().take(40).collect();
                tail.reverse();
                eprintln!("compositor/GTK log tail:\n{}", tail.join("\n"));
            }
        }
    }
}

#[test]
fn headless_control_socket_drives_input_screenshot_and_quit() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn();
    let first_stream = compositor.wait_for_socket();
    drop(first_stream);

    for command in [
        "pointer-motion-abs 10 10",
        "pointer-motion-rel 5 5",
        "pointer-button-down 272",
        "pointer-button-up 272",
        "axis 0 -120",
        "key-down 30",
        "key-up 30",
    ] {
        assert_eq!(
            compositor.send_text_command(command),
            "ok\n",
            "command {command:?}"
        );
    }

    let screenshot = compositor.take_screenshot();
    assert!(screenshot.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(png_dimensions(&screenshot), (WIDTH, HEIGHT));

    assert_eq!(compositor.send_text_command("quit"), "ok\n");
    wait_for_exit(&mut compositor.child);
}

#[cfg(feature = "test-apps")]
#[test]
fn headless_control_socket_spawns_and_drives_real_gtk_client() {
    let _guard = headless_test_guard();
    let _bus = session_bus::PrivateSessionBus::start().expect("private accessibility session");
    async_io::block_on(session_bus::enable_accessibility()).expect("enable accessibility");
    let mut compositor = HeadlessCompositor::spawn_with_size("800x600");
    let first_stream = compositor.wait_for_socket();
    drop(first_stream);

    let empty = compositor.take_screenshot();
    assert_eq!(png_dimensions(&empty), (CLIENT_WIDTH, CLIENT_HEIGHT));

    assert_eq!(compositor.send_text_command("spawn a11y-test"), "ok\n");
    let with_client = wait_for_screenshot_change(&compositor, &empty);
    assert_eq!(png_dimensions(&with_client), (CLIENT_WIDTH, CLIENT_HEIGHT));
    let lookup = wait_for_accessible_term("Research Workspace");
    let saw_accessible = lookup.as_ref().is_ok_and(|found| *found);
    if !saw_accessible {
        let diagnostics = format!(
            "lookup result: {lookup:?}\n{}",
            accessibility_diagnostics::lookup_failure_report()
        );
        compositor.runtime_dir.disable_cleanup(true);
        let path = compositor.runtime_dir.path().join("accessibility.log");
        std::fs::write(&path, &diagnostics).expect("save AT-SPI failure diagnostics");
        eprintln!(
            "AT-SPI lookup diagnostics ({}):\n{diagnostics}",
            path.display()
        );
    }
    assert!(lookup.is_ok(), "AT-SPI tree query failed: {lookup:?}");
    if std::env::var_os("HEARTHSPACE_REQUIRE_ATSPI").is_some() {
        assert!(saw_accessible, "AT-SPI tree did not expose GTK test app");
    } else if !saw_accessible {
        eprintln!(
            "GTK test app did not appear on AT-SPI bus; set HEARTHSPACE_REQUIRE_ATSPI=1 to make this fatal"
        );
    }

    for command in [
        "pointer-motion-abs 420 260",
        "pointer-button-down 272",
        "pointer-button-up 272",
        "key-down 30",
        "key-up 30",
        "screenshot",
    ] {
        if command == "screenshot" {
            assert_eq!(
                png_dimensions(&compositor.take_screenshot()),
                (CLIENT_WIDTH, CLIENT_HEIGHT)
            );
        } else {
            assert_eq!(
                compositor.send_text_command(command),
                "ok\n",
                "command {command:?}"
            );
        }
    }

    assert_eq!(compositor.send_text_command("quit"), "ok\n");
    wait_for_exit(&mut compositor.child);
}

fn send_text_command(command_socket_path: &Path, command: &str) -> String {
    let mut stream = UnixStream::connect(command_socket_path).expect("connect command socket");
    stream
        .write_all(format!("{command}\n").as_bytes())
        .expect("write command");
    read_line(&mut stream)
}

fn take_screenshot(command_socket_path: &Path) -> Vec<u8> {
    let mut stream = UnixStream::connect(command_socket_path).expect("connect command socket");
    stream.write_all(b"screenshot\n").expect("write screenshot");

    let header = read_line(&mut stream);
    let mut parts = header.split_whitespace();
    assert_eq!(parts.next(), Some("ok"));
    let len = parts
        .next()
        .expect("screenshot byte length")
        .parse::<usize>()
        .expect("valid screenshot byte length");
    assert_eq!(parts.next(), None);

    let mut payload = vec![0; len];
    stream.read_exact(&mut payload).expect("read PNG payload");
    payload
}

#[cfg(feature = "test-apps")]
fn wait_for_screenshot_change(compositor: &HeadlessCompositor, previous: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let screenshot = compositor.take_screenshot();
        if screenshot != previous {
            return screenshot;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("screenshot did not change after spawning GTK test client");
}

#[cfg(feature = "test-apps")]
fn wait_for_accessible_term(term: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if async_io::block_on(
            hearthspace_compositor::accessibility::accessibility_tree_contains_term(term),
        )? {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(false)
}

fn read_line(stream: &mut UnixStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        stream.read_exact(&mut byte).expect("read reply byte");
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    String::from_utf8(bytes).expect("utf-8 reply")
}

fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert!(png.len() >= 24);
    let width = u32::from_be_bytes(png[16..20].try_into().expect("PNG width bytes"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("PNG height bytes"));
    (width, height)
}

fn wait_for_exit(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if child.try_wait().expect("poll compositor").is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("headless Hearthspace did not exit after quit command");
}
