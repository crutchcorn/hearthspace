//! Owned headless fixture lifecycle and retained failure observations.
use super::*;

#[path = "fixture_diagnostics.rs"]
mod diagnostics;

pub(super) struct HeadlessCompositor {
    pub(super) child: Child,
    pub(super) runtime_dir: tempfile::TempDir,
    pub(super) command_socket_path: PathBuf,
    pub(super) _executable_dir: Option<tempfile::TempDir>,
    diagnostics: diagnostics::Diagnostics,
}

impl HeadlessCompositor {
    pub(super) fn spawn() -> Self {
        Self::spawn_with_size("320x240")
    }

    pub(super) fn spawn_with_size(size: &str) -> Self {
        Self::spawn_with_layout(size, None)
    }

    pub(super) fn spawn_with_layout(size: &str, layout: Option<&Path>) -> Self {
        Self::spawn_with_layout_and_path(size, layout, None)
    }

    pub(super) fn spawn_with_layout_and_path(
        size: &str,
        layout: Option<&Path>,
        path: Option<&Path>,
    ) -> Self {
        Self::spawn_with_options(size, layout, path, 1)
    }

    pub(super) fn spawn_with_options(
        size: &str,
        layout: Option<&Path>,
        path: Option<&Path>,
        scale: i32,
    ) -> Self {
        Self::spawn_with_remote_options(size, layout, path, scale, false)
    }

    pub(super) fn spawn_with_remote_options(
        size: &str,
        layout: Option<&Path>,
        path: Option<&Path>,
        scale: i32,
        remote_desktop: bool,
    ) -> Self {
        Self::spawn_with_command(size, layout, path, scale, remote_desktop, |_| {})
    }

    pub(super) fn spawn_with_command(
        size: &str,
        layout: Option<&Path>,
        path: Option<&Path>,
        scale: i32,
        remote_desktop: bool,
        configure: impl FnOnce(&mut Command),
    ) -> Self {
        Self::spawn_with_executable(
            size,
            layout,
            path,
            scale,
            remote_desktop,
            Path::new(env!("CARGO_BIN_EXE_hearthspace")),
            configure,
        )
    }

    pub(super) fn spawn_with_executable(
        size: &str,
        layout: Option<&Path>,
        path: Option<&Path>,
        scale: i32,
        remote_desktop: bool,
        executable: &Path,
        configure: impl FnOnce(&mut Command),
    ) -> Self {
        let runtime_dir = tempfile::tempdir().expect("create headless runtime dir");
        let command_socket_path = runtime_dir.path().join("command.sock");
        let wayland_display = format!("hearthspace-test-{}", std::process::id());
        let log = std::fs::File::create(runtime_dir.path().join("compositor.log"))
            .expect("create compositor log");
        let mut command = Command::new(executable);
        command
            .args(["--headless", "--no-shell", "--headless-size", size])
            .arg(format!("--headless-scale={scale}"))
            // Model operations use a process-independent cache lock. A mock
            // decision server must never contend with the user's live session.
            .env("XDG_CACHE_HOME", runtime_dir.path().join("cache"))
            .env("XDG_CONFIG_HOME", runtime_dir.path().join("config"))
            .env("XDG_DATA_HOME", runtime_dir.path().join("data"))
            .env("XDG_STATE_HOME", runtime_dir.path().join("state"))
            .env(
                "HEARTHSPACE_SETTINGS_SOCKET",
                runtime_dir.path().join("settings.sock"),
            )
            .env_remove("HEARTHSPACE_HOST_XDG_RUNTIME_DIR")
            .env_remove("WAYLAND_SOCKET")
            // Xwayland must not export hardware DMA-BUFs into software GLES.
            .env("XWAYLAND_NO_GLAMOR", "1")
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
            _executable_dir: None,
            diagnostics: diagnostics::Diagnostics::default(),
        }
    }

    pub(super) fn wait_for_socket(&mut self) -> UnixStream {
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

    pub(super) fn send_text_command(&self, command: &str) -> String {
        let reply = send_text_command(&self.command_socket_path, command);
        self.diagnostics.command(command, &reply);
        reply
    }

    pub(super) fn take_screenshot(&self) -> Vec<u8> {
        let frame = take_screenshot(&self.command_socket_path);
        self.diagnostics.frame(&frame);
        frame
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
            if let Err(error) = self.diagnostics.persist(self.runtime_dir.path()) {
                eprintln!("could not save fixture diagnostics: {error}");
            }
            eprintln!(
                "headless failure artifacts: {}",
                self.runtime_dir.path().display()
            );
            if let Ok(log) = diagnostics::log_tail(&self.runtime_dir.path().join("compositor.log"))
            {
                eprintln!("compositor/GTK log tail:\n{log}");
            }
        }
    }
}
