use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    process::{Child, Command},
};
use tokio_util::sync::CancellationToken;
use waydriver::{
    CaptureBackend, CompositorRuntime, Error, InputBackend, PipeWireStream, PointerAxis,
    PointerButton, Result, StreamToken, backend::cancellable_tail,
};

const WAYLAND_DISPLAY: &str = "wayland-99";
const COMMAND_SOCKET: &str = "hearthspace-shell.sock";
const DEFAULT_RESOLUTION: &str = "1280x720";
const INPUT_TAIL_DELAY: Duration = Duration::from_millis(30);
const PRESS_RELEASE_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct HearthspaceState {
    id: String,
    runtime_dir: PathBuf,
}

impl HearthspaceState {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    pub fn command_socket_path(&self) -> PathBuf {
        self.runtime_dir.join(COMMAND_SOCKET)
    }
}

pub struct HearthspaceCompositor {
    id: String,
    binary: PathBuf,
    runtime_dir: TempDir,
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
            runtime_dir,
            child: None,
            state: None,
        })
    }

    pub fn state(&self) -> Result<Arc<HearthspaceState>> {
        self.state
            .clone()
            .ok_or_else(|| Error::process("Hearthspace compositor has not been started"))
    }

    fn command_socket_path(&self) -> PathBuf {
        self.runtime_dir.path().join(COMMAND_SOCKET)
    }
}

#[async_trait]
impl CompositorRuntime for HearthspaceCompositor {
    async fn start(&mut self, resolution: Option<&str>, scale: Option<f64>) -> Result<()> {
        if self.child.is_some() {
            return Ok(());
        }

        let resolution = resolution.unwrap_or(DEFAULT_RESOLUTION);
        validate_resolution(resolution)?;
        let scale = scale_to_integer(scale)?;

        let mut command = Command::new(&self.binary);
        command
            .args(["--headless", "--no-shell", "--headless-size", resolution])
            .env("XDG_RUNTIME_DIR", self.runtime_dir.path())
            .env("WAYLAND_DISPLAY", WAYLAND_DISPLAY)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        if let Some(scale) = scale {
            command.args(["--headless-scale", &scale.to_string()]);
        }

        let child = command
            .spawn()
            .map_err(|e| Error::process_with("spawn hearthspace", e))?;
        self.child = Some(child);

        let command_socket_path = self.command_socket_path();
        wait_for_socket(self.child.as_mut(), &command_socket_path).await?;
        self.state = Some(Arc::new(HearthspaceState {
            id: self.id.clone(),
            runtime_dir: self.runtime_dir.path().to_path_buf(),
        }));

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
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

        self.state = None;
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn wayland_display(&self) -> &str {
        WAYLAND_DISPLAY
    }

    fn runtime_dir(&self) -> &Path {
        self.runtime_dir.path()
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

async fn wait_for_socket(child: Option<&mut Child>, path: &Path) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut child = child;
    let mut last_error = None;

    while tokio::time::Instant::now() < deadline {
        if let Some(child) = child.as_deref_mut() {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| Error::process_with("poll hearthspace", e))?
            {
                return Err(Error::process(format!(
                    "hearthspace exited before accepting commands: {status}"
                )));
            }
        }

        match UnixStream::connect(path).await {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error),
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    Err(Error::Timeout(format!(
        "timed out connecting to {}: {:?}",
        path.display(),
        last_error
    )))
}

async fn send_text_command(path: &Path, command: &str) -> Result<String> {
    let mut stream = UnixStream::connect(path)
        .await
        .map_err(|e| Error::process_with("connect command socket", e))?;
    stream
        .write_all(format!("{command}\n").as_bytes())
        .await
        .map_err(|e| Error::process_with("write command", e))?;

    let line = read_line(&mut stream).await?;
    if line == "ok\n" || line.starts_with("ok ") {
        Ok(line)
    } else if let Some(message) = line.strip_prefix("err ") {
        Err(Error::process(message.trim_end().to_string()))
    } else {
        Err(Error::process(format!("unexpected reply {line:?}")))
    }
}

async fn take_screenshot(path: &Path) -> Result<Vec<u8>> {
    let mut stream = UnixStream::connect(path)
        .await
        .map_err(|e| Error::screenshot_with("connect command socket", e))?;
    stream
        .write_all(b"screenshot\n")
        .await
        .map_err(|e| Error::screenshot_with("write command", e))?;

    let header = read_line(&mut stream).await?;
    let mut parts = header.split_whitespace();
    match parts.next() {
        Some("ok") => {}
        Some("err") => {
            let message = parts.collect::<Vec<_>>().join(" ");
            return Err(Error::screenshot(message));
        }
        _ => return Err(Error::screenshot(format!("unexpected reply {header:?}"))),
    }
    let len = parts
        .next()
        .ok_or_else(|| Error::screenshot("missing screenshot byte length"))?
        .parse::<usize>()
        .map_err(|e| Error::screenshot_with("parse screenshot byte length", e))?;
    if parts.next().is_some() {
        return Err(Error::screenshot(format!("unexpected reply {header:?}")));
    }

    let mut payload = vec![0; len];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| Error::screenshot_with("read screenshot payload", e))?;
    Ok(payload)
}

async fn read_line(stream: &mut UnixStream) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0];
        stream
            .read_exact(&mut byte)
            .await
            .map_err(|e| Error::process_with("read reply", e))?;
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    String::from_utf8(bytes).map_err(|e| Error::process_with("decode reply", e))
}

fn validate_resolution(resolution: &str) -> Result<()> {
    let Some((width, height)) = resolution
        .split_once('x')
        .or_else(|| resolution.split_once('X'))
    else {
        return Err(Error::process(format!(
            "invalid resolution {resolution:?}; expected WIDTHxHEIGHT"
        )));
    };
    for (label, value) in [("width", width), ("height", height)] {
        let value = value
            .parse::<i32>()
            .map_err(|e| Error::process_with(format!("parse {label}"), e))?;
        if value <= 0 {
            return Err(Error::process(format!("{label} must be positive")));
        }
    }
    Ok(())
}

fn scale_to_integer(scale: Option<f64>) -> Result<Option<i32>> {
    let Some(scale) = scale else {
        return Ok(None);
    };
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Error::process("scale must be a positive finite number"));
    }
    let rounded = scale.round();
    if (scale - rounded).abs() > f64::EPSILON {
        return Err(Error::process(format!(
            "Hearthspace headless scale must be an integer, got {scale}"
        )));
    }
    let scale =
        i32::try_from(rounded as i64).map_err(|e| Error::process_with("convert scale", e))?;
    Ok((scale != 1).then_some(scale))
}

fn keysym_to_evdev(keysym: u32) -> Result<u32> {
    if let Ok(byte) = u8::try_from(keysym) {
        if let Some(keycode) = ascii_keysym_to_evdev(byte) {
            return Ok(keycode);
        }
    }

    let keycode = match keysym {
        0xff08 => 14,  // BackSpace
        0xff09 => 15,  // Tab
        0xff0d => 28,  // Return
        0xff1b => 1,   // Escape
        0xffe1 => 42,  // Shift_L
        0xffe2 => 54,  // Shift_R
        0xffe3 => 29,  // Control_L
        0xffe4 => 97,  // Control_R
        0xffe9 => 56,  // Alt_L
        0xffea => 100, // Alt_R
        _ => {
            return Err(Error::process(format!(
                "unsupported keysym 0x{keysym:x}; add a keysym-to-evdev mapping"
            )));
        }
    };
    Ok(keycode)
}

fn ascii_keysym_to_evdev(keysym: u8) -> Option<u32> {
    Some(match keysym.to_ascii_lowercase() {
        b'a' => 30,
        b'b' => 48,
        b'c' => 46,
        b'd' => 32,
        b'e' => 18,
        b'f' => 33,
        b'g' => 34,
        b'h' => 35,
        b'i' => 23,
        b'j' => 36,
        b'k' => 37,
        b'l' => 38,
        b'm' => 50,
        b'n' => 49,
        b'o' => 24,
        b'p' => 25,
        b'q' => 16,
        b'r' => 19,
        b's' => 31,
        b't' => 20,
        b'u' => 22,
        b'v' => 47,
        b'w' => 17,
        b'x' => 45,
        b'y' => 21,
        b'z' => 44,
        b'1' => 2,
        b'2' => 3,
        b'3' => 4,
        b'4' => 5,
        b'5' => 6,
        b'6' => 7,
        b'7' => 8,
        b'8' => 9,
        b'9' => 10,
        b'0' => 11,
        b' ' => 57,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_must_be_integer() {
        assert_eq!(scale_to_integer(None).unwrap(), None);
        assert_eq!(scale_to_integer(Some(1.0)).unwrap(), None);
        assert_eq!(scale_to_integer(Some(2.0)).unwrap(), Some(2));
        assert!(scale_to_integer(Some(1.5)).is_err());
    }

    #[test]
    fn maps_common_keysyms_to_evdev() {
        assert_eq!(keysym_to_evdev(b'a' as u32).unwrap(), 30);
        assert_eq!(keysym_to_evdev(b'A' as u32).unwrap(), 30);
        assert_eq!(keysym_to_evdev(0xff0d).unwrap(), 28);
        assert!(keysym_to_evdev(0x2603).is_err());
    }

    #[test]
    fn validates_resolution_shape() {
        assert!(validate_resolution("800x600").is_ok());
        assert!(validate_resolution("800X600").is_ok());
        assert!(validate_resolution("800").is_err());
        assert!(validate_resolution("0x600").is_err());
    }
}
