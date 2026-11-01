use super::*;
use smithay::xwayland::X11Wm;
use std::{
    io::{Read, Write},
    sync::Arc,
};
use wayland_server::Display;

#[test]
fn startup_deadline_cannot_be_extended_by_unrelated_event_loop_activity() {
    let dependency = Dependency::current().unwrap();
    let display: Display<App> = Display::new().unwrap();
    let (socket, _peer) = UnixStream::pair().unwrap();
    let client = display
        .handle()
        .insert_client(
            socket,
            Arc::new(crate::compositor::client::ClientState::default()),
        )
        .unwrap();
    let mut startup = Startup {
        dependency,
        client,
        deadline: Instant::now() - Duration::from_secs(1),
        ready: false,
        failure: None,
    };
    let error = startup.remaining().unwrap_err().to_string();
    assert!(error.contains("timed out"));
    assert!(error.contains("hearthspace-Xwayland.json"));
    startup.failure = Some("Xwayland exited during startup".into());
    assert!(
        startup
            .remaining()
            .unwrap_err()
            .to_string()
            .contains("exited during startup")
    );
    startup.failure = Some("failed to initialize XWM".into());
    assert!(
        startup
            .remaining()
            .unwrap_err()
            .to_string()
            .contains("failed to initialize XWM")
    );
}

#[test]
fn deadline_interrupts_real_smithay_xwm_handshake_when_peer_never_replies() {
    let display: Display<App> = Display::new().unwrap();
    let mut dh = display.handle();
    let (wayland, _wayland_peer) = UnixStream::pair().unwrap();
    let client = dh
        .insert_client(
            wayland,
            Arc::new(crate::compositor::client::ClientState::default()),
        )
        .unwrap();
    let (socket, _silent_peer) = UnixStream::pair().unwrap();
    let started = Instant::now();
    let _guard = ConnectionDeadline::new(&socket, started + Duration::from_millis(75)).unwrap();
    let events: EventLoop<'static, App> = EventLoop::try_new().unwrap();
    assert!(X11Wm::start_wm(events.handle(), &dh, socket, client).is_err());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "XWM handshake escaped its deadline"
    );
}

#[test]
fn successful_initialization_cancels_watchdog_before_connection_is_reused() {
    let (mut socket, mut peer) = UnixStream::pair().unwrap();
    let guard =
        ConnectionDeadline::new(&socket, Instant::now() + Duration::from_millis(25)).unwrap();
    drop(guard);
    thread::sleep(Duration::from_millis(50));
    peer.write_all(b"still alive").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut bytes = [0; 11];
    socket.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"still alive");
}
