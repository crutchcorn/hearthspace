use std::{path::PathBuf, sync::Arc};

use tokio_util::sync::CancellationToken;
use waydriver::{CaptureBackend, CompositorRuntime, InputBackend, PointerAxis, PointerButton};
#[cfg(feature = "test-apps")]
use waydriver::{Session, SessionConfig};
use waydriver_hearthspace::{HearthspaceCapture, HearthspaceCompositor, HearthspaceInput};

#[cfg(feature = "test-apps")]
const GTK_TEST_APP_ACCESSIBLE_NAME: &str = "hearthspace-gtk-test-app";

static WAYDRIVER_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
#[ignore = "requires surfaceless EGL and the headless compositor socket"]
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

#[cfg(feature = "test-apps")]
#[tokio::test]
#[ignore = "requires surfaceless EGL, GTK, and AT-SPI exposure"]
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
                video_output: None,
                video_bitrate: None,
                video_fps: None,
                prewarm_visual: false,
                visual_region_tuning: Default::default(),
                visual_text_tuning: Default::default(),
                visual_click_tuning: Default::default(),
                gsettings_isolated: true,
                xdg_isolated: true,
                extra_env: vec![("GDK_BACKEND".to_string(), "wayland".to_string())],
                capture_external_effects: false,
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

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

fn png_dimensions(png: &[u8]) -> (u32, u32) {
    assert!(png.len() >= 24);
    let width = u32::from_be_bytes(png[16..20].try_into().expect("PNG width bytes"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("PNG height bytes"));
    (width, height)
}
