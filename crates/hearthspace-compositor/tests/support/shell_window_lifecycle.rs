//! Layout editing creates and retires transient shell windows inside callbacks.
//! Exercise the shell's actual Winit dependency against an owned Wayland server.
use std::{env, fs, os::unix::process::CommandExt};

use smithay::reexports::rustix::process::{Pid, Signal, kill_process_group};
use xilem::winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::wayland::EventLoopBuilderExtWayland,
    window::{Window, WindowId},
};

use super::*;

const CHILD_ENV: &str = "HEARTHSPACE_TEST_TRANSIENT_WINDOW";
const CHILD_TEST: &str = "shell_window_lifecycle::wayland_window_lifecycle_child";

struct OwnedClient(Child);

impl Drop for OwnedClient {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            // The child has its own group and has not been reaped, so this
            // cannot target the test runner or a reused process-group ID.
            let _ = kill_process_group(Pid::from_raw(self.0.id() as i32).unwrap(), Signal::KILL);
            let _ = self.0.wait();
        }
    }
}

#[test]
fn transient_shell_window_retired_in_callback_preserves_survivor_resize() {
    let _guard = headless_test_guard();
    let mut compositor =
        HeadlessCompositor::spawn_with_command("800x600", None, None, 1, false, |command| {
            command
                .env("LIBGL_ALWAYS_SOFTWARE", "1")
                .env("VK_DRIVER_FILES", "/usr/share/vulkan/icd.d/lvp_icd.json")
                .env("GSK_RENDERER", "cairo")
                .env_remove("DBUS_SESSION_BUS_ADDRESS")
                .env_remove("DBUS_SYSTEM_BUS_ADDRESS")
                .env_remove("AT_SPI_BUS_ADDRESS")
                .env_remove("HEARTHSPACE_SETTINGS_SOCKET")
                .env_remove("HEARTHSPACE_LOG_FILE");
        });
    drop(compositor.wait_for_socket());
    let log_path = compositor.runtime_dir.path().join("window-lifecycle.log");
    let log = fs::File::create(&log_path).unwrap();
    let mut client = OwnedClient(
        Command::new(env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .env_clear()
            .env(CHILD_ENV, "1")
            .env("XDG_RUNTIME_DIR", compositor.runtime_dir.path())
            .env(
                "WAYLAND_DISPLAY",
                format!("hearthspace-test-{}", std::process::id()),
            )
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = client.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "Wayland lifecycle client timed out:\n{}",
            fs::read_to_string(&log_path).unwrap()
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "Wayland lifecycle client failed ({status}):\n{}",
        fs::read_to_string(log_path).unwrap()
    );
    let mut control = UnixStream::connect(&compositor.command_socket_path).unwrap();
    control
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    control
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    control.write_all(b"quit\n").unwrap();
    assert_eq!(read_line(&mut control), "ok\n");
    wait_for_exit(&mut compositor.child);
}

#[derive(Default)]
struct Lifecycle {
    survivor: Option<Window>,
    initial_size: Option<PhysicalSize<u32>>,
    retired: Option<WindowId>,
    destroyed: bool,
    resize_requested: bool,
    resized_after_retirement: bool,
}

impl ApplicationHandler for Lifecycle {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.survivor = Some(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Persistent shell window")
                        .with_decorations(false)
                        .with_inner_size(LogicalSize::new(240, 120)),
                )
                .unwrap(),
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) == self.retired {
            assert!(
                matches!(event, WindowEvent::Destroyed),
                "retired transient received a stale event: {event:?}"
            );
            assert!(!self.destroyed, "transient was destroyed twice");
            self.destroyed = true;
            return;
        }
        let survivor = self.survivor.as_ref().unwrap();
        assert_eq!(window_id, survivor.id());
        if let WindowEvent::Resized(size) = event {
            if self.initial_size.is_none() {
                self.initial_size = Some(size);
                // Winit's creation roundtrip queues this window's initial
                // configure while it is already dispatching survivor events.
                // Shell view updates can retire the window in that callback.
                let transient = event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Retired layout hint")
                            .with_decorations(false),
                    )
                    .unwrap();
                self.retired = Some(transient.id());
                drop(transient);
            } else if self.resize_requested && Some(size) != self.initial_size {
                assert!(survivor.is_maximized());
                self.resized_after_retirement = true;
                event_loop.exit();
            }
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if self.destroyed && !self.resize_requested {
            self.resize_requested = true;
            // Require a later real compositor configure on the survivor;
            // simply reaching Destroyed would miss the next-iteration panic.
            self.survivor.as_ref().unwrap().set_maximized(true);
        }
    }
}

#[test]
fn wayland_window_lifecycle_child() {
    // A separate process gives Winit its one event loop without mutating the
    // parallel test runner's display environment. Normal suite runs skip it.
    if env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let event_loop = EventLoop::builder()
        .with_wayland()
        .with_any_thread(true)
        .build()
        .unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut lifecycle = Lifecycle::default();
    event_loop.run_app(&mut lifecycle).unwrap();
    assert!(lifecycle.destroyed);
    assert!(lifecycle.resized_after_retirement);
}
