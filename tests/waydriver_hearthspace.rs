use std::{
    ffi::OsString,
    io::BufRead,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
};

use tokio_util::sync::CancellationToken;
use waydriver::{CaptureBackend, CompositorRuntime, InputBackend, PointerAxis, PointerButton};
use waydriver::{Session, SessionConfig};
use waydriver_hearthspace::{HearthspaceCapture, HearthspaceCompositor, HearthspaceInput};

#[cfg(feature = "test-apps")]
const GTK_TEST_APP_ACCESSIBLE_NAME: &str = "hearthspace-gtk-test-app";
const SHELL_ACCESSIBLE_NAME: &str = "hearthspace";

static WAYDRIVER_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn waydriver_backends_drive_input_capture_and_teardown() {
    init_tracing();
    let _guard = WAYDRIVER_TEST_LOCK.lock().await;
    let mut compositor = HearthspaceCompositor::new(hearthspace_binary()).unwrap();
    compositor.start(Some("320x240"), Some(1.0)).await.unwrap();

    let state = compositor.state().unwrap();
    let input = HearthspaceInput::new(Arc::clone(&state));
    let capture = HearthspaceCapture::new(state);
    let cancel = CancellationToken::new();

    input
        .pointer_motion_absolute(10.0, 10.0, &cancel)
        .await
        .unwrap();
    input
        .pointer_motion_relative(5.0, 5.0, &cancel)
        .await
        .unwrap();
    input
        .pointer_button(PointerButton::Left, &cancel)
        .await
        .unwrap();
    input
        .pointer_axis_discrete(PointerAxis::Vertical, -1, &cancel)
        .await
        .unwrap();
    input.press_keysym(b'a' as u32, &cancel).await.unwrap();

    let stream = capture.start_stream().await.unwrap();
    let screenshot = capture.grab_screenshot(&stream).await.unwrap();
    assert!(screenshot.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(png_dimensions(&screenshot), (320, 240));
    capture.stop_stream(stream).await.unwrap();

    compositor.stop().await.unwrap();
}

#[tokio::test]
async fn waydriver_session_locates_xilem_shell_by_xpath() -> Result<(), Box<dyn std::error::Error>>
{
    init_tracing();
    let _guard = WAYDRIVER_TEST_LOCK.lock().await;
    let _bus = PrivateSessionBus::start()?;
    set_screen_reader_enabled(true).await?;
    run_xilem_shell_xpath_check().await
}

async fn run_xilem_shell_xpath_check() -> Result<(), Box<dyn std::error::Error>> {
    let mut compositor = HearthspaceCompositor::new(hearthspace_binary())?.with_shell();
    compositor.start(Some("800x600"), Some(1.0)).await?;

    let state = compositor.state()?;
    let input = Box::new(HearthspaceInput::new(Arc::clone(&state)));
    let capture = Box::new(HearthspaceCapture::new(state));
    let compositor = Box::new(compositor);
    let session = Arc::new(
        Session::start(
            compositor,
            input,
            capture,
            session_config("true", vec![], SHELL_ACCESSIBLE_NAME),
        )
        .await?,
    );

    let check_result = async {
        let pan_left = session.locate("//*[@name='LEFT']").first();
        let name = pan_left.name().await?;
        if name.as_deref() != Some("LEFT") {
            return Err(std::io::Error::other(format!(
                "expected LEFT shell control, got {name:?}"
            ))
            .into());
        }
        let bounds = pan_left.bounds().await?;
        if bounds.width <= 0 || bounds.height <= 0 {
            return Err(std::io::Error::other(format!(
                "LEFT shell control has invalid bounds {bounds:?}"
            ))
            .into());
        }
        Ok(())
    }
    .await;

    let session = Arc::try_unwrap(session)
        .map_err(|_| std::io::Error::other("session still has live references"))?;
    session.kill().await?;
    check_result
}

#[cfg(feature = "test-apps")]
#[tokio::test]
async fn waydriver_session_locates_real_client_by_xpath() {
    init_tracing();
    let _guard = WAYDRIVER_TEST_LOCK.lock().await;

    let mut compositor = HearthspaceCompositor::new(hearthspace_binary()).unwrap();
    compositor.start(Some("800x600"), Some(1.0)).await.unwrap();

    let state = compositor.state().unwrap();
    let input = Box::new(HearthspaceInput::new(Arc::clone(&state)));
    let capture = Box::new(HearthspaceCapture::new(state));
    let compositor = Box::new(compositor);
    let session = Arc::new(
        Session::start(
            compositor,
            input,
            capture,
            SessionConfig {
                command: hearthspace_binary().to_string_lossy().into_owned(),
                args: vec!["--gtk-test-app".to_string()],
                cwd: None,
                // GTK exposes the AT-SPI application root using argv[0], not
                // the window title or application id.
                app_name: GTK_TEST_APP_ACCESSIBLE_NAME.to_string(),
                extra_env: vec![("GDK_BACKEND".to_string(), "wayland".to_string())],
                ..session_config("", vec![], "")
            },
        )
        .await
        .unwrap(),
    );

    {
        let header = session.locate("//*[@name='Research Workspace']");
        header.click().await.unwrap();
    }
    let screenshot = session.take_screenshot().await.unwrap();
    assert_eq!(png_dimensions(&screenshot), (800, 600));
    let session = match Arc::try_unwrap(session) {
        Ok(session) => session,
        Err(_) => panic!("session still has live references"),
    };
    session.kill().await.unwrap();
}

fn hearthspace_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_hearthspace"))
}

fn session_config(
    command: impl Into<String>,
    args: Vec<String>,
    app_name: impl Into<String>,
) -> SessionConfig {
    SessionConfig {
        command: command.into(),
        args,
        cwd: None,
        app_name: app_name.into(),
        video_output: None,
        video_bitrate: None,
        video_fps: None,
        prewarm_visual: false,
        visual_region_tuning: Default::default(),
        visual_text_tuning: Default::default(),
        visual_click_tuning: Default::default(),
        gsettings_isolated: true,
        xdg_isolated: true,
        extra_env: vec![],
        capture_external_effects: false,
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

struct PrivateSessionBus {
    previous_address: Option<OsString>,
    child: Child,
}

impl PrivateSessionBus {
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let previous_address = std::env::var_os("DBUS_SESSION_BUS_ADDRESS");
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("dbus-daemon stdout was not piped"))?;
        let mut reader = std::io::BufReader::new(stdout);
        let mut address = String::new();
        reader.read_line(&mut address)?;
        let address = address.trim();
        if address.is_empty() {
            return Err(std::io::Error::other("dbus-daemon did not print an address").into());
        }

        // SAFETY: these ignored WayDriver tests are serialized by
        // WAYDRIVER_TEST_LOCK before this guard is created. The env var is
        // restored in Drop before the lock is released, so no other test in this
        // process observes the private bus address.
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address);
        }

        Ok(Self {
            previous_address,
            child,
        })
    }
}

impl Drop for PrivateSessionBus {
    fn drop(&mut self) {
        match &self.previous_address {
            Some(address) => {
                // SAFETY: see PrivateSessionBus::start; the same test lock is
                // still held while this guard is dropped.
                unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address) };
            }
            None => {
                // SAFETY: see PrivateSessionBus::start; the same test lock is
                // still held while this guard is dropped.
                unsafe { std::env::remove_var("DBUS_SESSION_BUS_ADDRESS") };
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn set_screen_reader_enabled(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    let connection = atspi::zbus::Connection::session().await?;
    let status = atspi::proxy::bus::StatusProxy::new(&connection).await?;
    status.set_screen_reader_enabled(enabled).await?;
    Ok(())
}

fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert!(png.len() >= 24);
    let width = u32::from_be_bytes(png[16..20].try_into().expect("PNG width bytes"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("PNG height bytes"));
    (width, height)
}
