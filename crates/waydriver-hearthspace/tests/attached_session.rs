//! Opt-in integration against the built compositor, without changing host sessions.

use std::{path::PathBuf, process::Stdio, time::Duration};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    time::{Instant, sleep, timeout},
};
use waydriver::{CaptureBackend, CompositorRuntime};
use waydriver_hearthspace::{
    HearthspaceCapture, HearthspaceCompositor, SessionOwnership,
    accessibility::SessionAccessibility,
};

#[tokio::test]
async fn attach_discovers_shell_on_private_bus_and_preserves_the_desktop() {
    if std::env::var("HEARTHSPACE_E2E").as_deref() != Ok("1") {
        eprintln!("set HEARTHSPACE_E2E=1 to test against the built compositor");
        return;
    }
    let binary = std::env::var_os("HEARTHSPACE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/hearthspace")
        });
    let runtime = tempfile::tempdir().unwrap();
    let socket = runtime.path().join("attached.sock");

    let mut bus = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(bus.stdout.take().unwrap()).lines();
    let address = timeout(Duration::from_secs(5), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Activate accessibility on this private bus before the real shell client
    // starts. Do not alter the test process's host bus environment.
    let enabled = timeout(
        Duration::from_secs(5),
        Command::new("gdbus")
            .args([
                "call",
                "--address",
                &address,
                "--dest",
                "org.a11y.Bus",
                "--object-path",
                "/org/a11y/bus",
                "--method",
                "org.freedesktop.DBus.Properties.Set",
                "org.a11y.Status",
                "ScreenReaderEnabled",
                "<true>",
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        enabled.status.success(),
        "{}",
        String::from_utf8_lossy(&enabled.stderr)
    );

    let mut desktop = Command::new(binary)
        .args(["--headless", "--headless-size", "800x600"])
        .env("XDG_RUNTIME_DIR", runtime.path())
        .env("HEARTHSPACE_WAYLAND_DISPLAY", "wayland-attached")
        .env("HEARTHSPACE_COMMAND_SOCKET", &socket)
        .env("DBUS_SESSION_BUS_ADDRESS", &address)
        .env("HEARTHSPACE_SESSION_BUS_ADDRESS", &address)
        .env_remove("AT_SPI_BUS_ADDRESS")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let mut adapter = HearthspaceCompositor::attach(runtime.path(), "wayland-attached", &socket);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if adapter.start(None, None).await.is_ok() {
            break;
        }
        assert!(
            desktop.try_wait().unwrap().is_none(),
            "desktop exited during startup"
        );
        assert!(Instant::now() < deadline, "desktop startup timed out");
        sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(adapter.ownership(), SessionOwnership::Attached);
    let accessibility = SessionAccessibility::new(&address).unwrap();
    let selector = "//*[@name='LEFT' and @focusable='true']";
    loop {
        let matches = accessibility
            .find(selector, Duration::from_secs(5))
            .await
            .unwrap();
        if !matches.is_empty() {
            assert_eq!(matches.len(), 1);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "shell accessibility startup timed out"
        );
        sleep(Duration::from_millis(50)).await;
    }
    accessibility
        .activate(selector, Duration::from_secs(5))
        .await
        .unwrap();
    let capture = HearthspaceCapture::new(adapter.state().unwrap());
    let stream = capture.start_stream().await.unwrap();
    assert!(
        capture
            .grab_screenshot(&stream)
            .await
            .unwrap()
            .starts_with(b"\x89PNG")
    );

    adapter.stop().await.unwrap();
    drop(adapter);
    assert!(
        desktop.try_wait().unwrap().is_none(),
        "detach killed an external compositor"
    );
    assert!(socket.exists(), "detach removed external runtime files");
    assert!(
        capture
            .grab_screenshot(&stream)
            .await
            .unwrap()
            .starts_with(b"\x89PNG")
    );

    desktop.kill().await.unwrap();
    bus.kill().await.unwrap();
}
