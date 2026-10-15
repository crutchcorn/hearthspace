use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;
use waydriver::{
    CaptureBackend, CompositorRuntime, Error, InputBackend, PipeWireStream, PointerAxis,
    PointerButton, Result, StreamToken, backend::cancellable_tail,
};

mod config;
mod keymap;
mod transport;

pub mod accessibility;

use config::{scale_to_integer, validate_resolution};
use keymap::keysym_to_evdev;
use transport::{send_text_command, take_screenshot, wait_for_socket};

const WAYLAND_DISPLAY: &str = "wayland-99";
const COMMAND_SOCKET: &str = "hearthspace-shell.sock";
const DEFAULT_RESOLUTION: &str = "1280x720";
const INPUT_TAIL_DELAY: Duration = Duration::from_millis(30);
const PRESS_RELEASE_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct HearthspaceState {
    id: String,
    runtime_dir: PathBuf,
    command_socket: PathBuf,
}

impl HearthspaceState {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    pub fn command_socket_path(&self) -> PathBuf {
        self.command_socket.clone()
    }
}

/// Whether the adapter may stop the compositor and remove its runtime files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionOwnership {
    Owned,
    Attached,
}

pub struct HearthspaceCompositor {
    id: String,
    binary: PathBuf,
    start_shell: bool,
    runtime_dir: PathBuf,
    owned_runtime: Option<TempDir>,
    command_socket: PathBuf,
    wayland_display: String,
    child: Option<Child>,
    state: Option<Arc<HearthspaceState>>,
}

impl HearthspaceCompositor {
    pub fn new(binary: impl Into<PathBuf>) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let runtime_dir = tempfile::Builder::new()
            .prefix(&format!("wd-hearthspace-{id}-"))
            .tempdir()
            .map_err(|e| Error::process_with("create runtime dir", e))?;

        Ok(Self {
            id,
            binary: binary.into(),
            start_shell: false,
            runtime_dir: runtime_dir.path().to_path_buf(),
            command_socket: runtime_dir.path().join(COMMAND_SOCKET),
            owned_runtime: Some(runtime_dir),
            wayland_display: WAYLAND_DISPLAY.into(),
            child: None,
            state: None,
        })
    }

    /// Attach to an explicitly selected existing session. `start` checks its
    /// control socket; `stop` only detaches. Dropping this adapter never kills
    /// the external session or removes its files. Display configuration is owned
    /// by that session, so `start` rejects resolution/scale overrides.
    pub fn attach(
        runtime_dir: impl Into<PathBuf>,
        wayland_display: impl Into<String>,
        command_socket: impl Into<PathBuf>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            binary: PathBuf::new(),
            start_shell: false,
            runtime_dir: runtime_dir.into(),
            owned_runtime: None,
            command_socket: command_socket.into(),
            wayland_display: wayland_display.into(),
            child: None,
            state: None,
        }
    }

    pub fn ownership(&self) -> SessionOwnership {
        if self.owned_runtime.is_some() {
            SessionOwnership::Owned
        } else {
            SessionOwnership::Attached
        }
    }

    fn make_state(&mut self) {
        self.state = Some(Arc::new(HearthspaceState {
            id: self.id.clone(),
            runtime_dir: self.runtime_dir.clone(),
            command_socket: self.command_socket.clone(),
        }));
    }

    pub fn with_shell(mut self) -> Self {
        self.start_shell = true;
        self
    }

    pub fn state(&self) -> Result<Arc<HearthspaceState>> {
        self.state
            .clone()
            .ok_or_else(|| Error::process("Hearthspace compositor has not been started"))
    }

    fn command_socket_path(&self) -> PathBuf {
        self.command_socket.clone()
    }
}

#[async_trait]
impl CompositorRuntime for HearthspaceCompositor {
    async fn start(&mut self, resolution: Option<&str>, scale: Option<f64>) -> Result<()> {
        if self.ownership() == SessionOwnership::Attached {
            if resolution.is_some() || scale.is_some() {
                return Err(Error::process("cannot reconfigure an attached session"));
            }
            send_text_command(&self.command_socket, "status").await?;
            self.make_state();
            return Ok(());
        }
        if self.state.is_some() {
            return Ok(());
        }

        let resolution = resolution.unwrap_or(DEFAULT_RESOLUTION);
        validate_resolution(resolution)?;
        let scale = scale_to_integer(scale)?;

        let mut command = Command::new(&self.binary);
        command
            .args(["--headless", "--headless-size", resolution])
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .env("WAYLAND_DISPLAY", &self.wayland_display)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        if !self.start_shell {
            command.arg("--no-shell");
        }

        if let Some(scale) = scale {
            command.args(["--headless-scale", &scale.to_string()]);
        }

        let child = command
            .spawn()
            .map_err(|e| Error::process_with("spawn hearthspace", e))?;
        self.child = Some(child);

        let command_socket_path = self.command_socket_path();
        if let Err(error) = wait_for_socket(self.child.as_mut(), &command_socket_path).await {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill().await;
            }
            return Err(error);
        }
        self.make_state();

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.state = None;
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };

        if child
            .try_wait()
            .map_err(|e| Error::process_with("poll hearthspace", e))?
            .is_none()
        {
            let _ = send_text_command(&self.command_socket_path(), "quit").await;
            if tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .is_err()
            {
                child
                    .kill()
                    .await
                    .map_err(|e| Error::process_with("kill hearthspace", e))?;
                let _ = child.wait().await;
            }
        }

        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn wayland_display(&self) -> &str {
        &self.wayland_display
    }

    fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }
}

pub struct HearthspaceInput {
    state: Arc<HearthspaceState>,
}

impl HearthspaceInput {
    pub fn new(state: Arc<HearthspaceState>) -> Self {
        Self { state }
    }

    async fn send(&self, command: impl AsRef<str>) -> Result<()> {
        send_text_command(&self.state.command_socket_path(), command.as_ref()).await?;
        Ok(())
    }
}

#[async_trait]
impl InputBackend for HearthspaceInput {
    async fn press_keysym(&self, keysym: u32, cancel: &CancellationToken) -> Result<()> {
        self.key_down(keysym, cancel).await?;
        tokio::time::sleep(PRESS_RELEASE_DELAY).await;
        self.key_up(keysym, cancel).await?;
        cancellable_tail(INPUT_TAIL_DELAY, cancel).await;
        Ok(())
    }

    async fn key_down(&self, keysym: u32, _cancel: &CancellationToken) -> Result<()> {
        let keycode = keysym_to_evdev(keysym)?;
        self.send(format!("key-down {keycode}")).await
    }

    async fn key_up(&self, keysym: u32, _cancel: &CancellationToken) -> Result<()> {
        let keycode = keysym_to_evdev(keysym)?;
        self.send(format!("key-up {keycode}")).await
    }

    async fn pointer_motion_relative(
        &self,
        dx: f64,
        dy: f64,
        _cancel: &CancellationToken,
    ) -> Result<()> {
        self.send(format!("pointer-motion-rel {dx} {dy}")).await
    }

    async fn pointer_motion_absolute(
        &self,
        x: f64,
        y: f64,
        _cancel: &CancellationToken,
    ) -> Result<()> {
        self.send(format!("pointer-motion-abs {x} {y}")).await
    }

    async fn pointer_button_down(
        &self,
        button: PointerButton,
        _cancel: &CancellationToken,
    ) -> Result<()> {
        self.send(format!("pointer-button-down {}", button.evdev_code()))
            .await
    }

    async fn pointer_button_up(
        &self,
        button: PointerButton,
        cancel: &CancellationToken,
    ) -> Result<()> {
        self.send(format!("pointer-button-up {}", button.evdev_code()))
            .await?;
        cancellable_tail(INPUT_TAIL_DELAY, cancel).await;
        Ok(())
    }

    async fn pointer_axis_discrete(
        &self,
        axis: PointerAxis,
        steps: i32,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let (horizontal, vertical) = match axis {
            PointerAxis::Vertical => (0, steps * 120),
            PointerAxis::Horizontal => (steps * 120, 0),
        };
        self.send(format!("axis {horizontal} {vertical}")).await?;
        cancellable_tail(INPUT_TAIL_DELAY, cancel).await;
        Ok(())
    }
}

pub struct HearthspaceCapture {
    state: Arc<HearthspaceState>,
}

impl HearthspaceCapture {
    pub fn new(state: Arc<HearthspaceState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl CaptureBackend for HearthspaceCapture {
    async fn start_stream(&self) -> Result<PipeWireStream> {
        Ok(PipeWireStream {
            node_id: 0,
            token: StreamToken::new(()),
        })
    }

    async fn stop_stream(&self, _stream: PipeWireStream) -> Result<()> {
        Ok(())
    }

    fn pipewire_socket(&self) -> PathBuf {
        self.state.runtime_dir().join("pipewire-0")
    }

    async fn grab_screenshot(&self, _stream: &PipeWireStream) -> Result<Vec<u8>> {
        take_screenshot(&self.state.command_socket_path()).await
    }
}

#[cfg(test)]
mod tests;
