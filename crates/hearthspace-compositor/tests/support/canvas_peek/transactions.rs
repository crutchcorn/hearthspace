//! Placement and asynchronous resize are atomic with respect to canvas peeking.
use super::*;
use wayland_protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;

#[test]
fn canvas_peek_persisted_launch_does_not_settle_at_the_provisional_origin() {
    let _guard = headless_test_guard();
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("layout.json");
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let (_, left) = normal(
        &compositor,
        &mut client,
        "dev.peek.ProvisionalLeft",
        (180, 110),
        (800, 100),
    );
    let (_, right) = normal(
        &compositor,
        &mut client,
        "dev.peek.ProvisionalRight",
        (180, 110),
        (1000, 100),
    );
    action(&compositor, format!("move-window {left} 20 20"));
    action(&compositor, format!("move-window {right} 220 20"));
    let before = [
        geometry(&window(&compositor, left)),
        geometry(&window(&compositor, right)),
    ];
    let viewport = query(&compositor, "status")["viewport"].clone();

    // At (0,0), this new app covers both neighbors completely. Its actual
    // centered placement starts at x=430, beyond either neighbor's right edge.
    client.add_decorated(
        "dev.peek.PersistedLaunch".into(),
        (420, 280),
        COLOR,
        Mode::ServerSide,
    );
    let windows = wait_windows(&compositor, |windows| windows.len() == 3);
    let launched = windows
        .iter()
        .find(|w| w["app_id"] == "dev.peek.PersistedLaunch")
        .unwrap();
    super::super::window_placement::assert_centered(launched, 1280.0, 800.0);
    assert!(launched["canvas_x"].as_i64().unwrap() >= before[1][0] + before[1][2]);
    assert_eq!(geometry(&window(&compositor, left)), before[0]);
    assert_eq!(geometry(&window(&compositor, right)), before[1]);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);

    shutdown(compositor);
    assert!(
        path.is_file(),
        "the regression must exercise enabled persistence"
    );
}

fn resize_from_top_left(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    id: u64,
) -> (f64, f64) {
    // The initial client buffer is 240px wide; SSD has a 260px minimum
    // outer width. Resizing starts from that visible outer rectangle.
    let foreground = window(compositor, id);
    let point = (
        foreground["x"].as_f64().unwrap() + 120.0,
        foreground["y"].as_f64().unwrap() + 100.0,
    );
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
    action(compositor, "pointer-button-down 272");
    let serial = client.press_serial();
    client.request_resize_edge(1, serial, ResizeEdge::TopLeft);
    point
}

fn resize_motion(compositor: &HeadlessCompositor, point: (f64, f64), delta: f64) {
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0 + delta, point.1 + delta),
    );
}

#[test]
fn canvas_peek_resize_release_waits_for_the_delayed_final_shrink() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let (mut client, [covered, foreground]) = interactive(&compositor);
    action(&compositor, format!("move-window {foreground} 500 500"));
    let original = geometry(&window(&compositor, covered));
    let point = resize_from_top_left(&compositor, &mut client, foreground);

    resize_motion(&compositor, point, -300.0);
    client.sync();
    assert_eq!(
        geometry(&window(&compositor, foreground)),
        [200, 200, 560, 460]
    );
    assert_eq!(geometry(&window(&compositor, covered)), original);

    // Leave both the shrink and the final release configure undispatched.
    // The large intermediate buffer remains visible, but must never settle.
    resize_motion(&compositor, point, 0.0);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(
        geometry(&window(&compositor, foreground)),
        [200, 200, 560, 460]
    );
    assert_eq!(geometry(&window(&compositor, covered)), original);

    client.sync();
    assert_eq!(
        geometry(&window(&compositor, foreground)),
        [500, 500, 260, 160]
    );
    assert_eq!(geometry(&window(&compositor, covered)), original);
    assert!(!client.configuration(1).1);
    shutdown(compositor);
}

#[test]
fn canvas_peek_delayed_final_resize_commit_reanchors_then_settles() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let (mut client, [covered, foreground]) = interactive(&compositor);
    action(&compositor, format!("move-window {foreground} 500 500"));
    let original = geometry(&window(&compositor, covered));
    let point = resize_from_top_left(&compositor, &mut client, foreground);

    resize_motion(&compositor, point, -300.0);
    action(&compositor, "pointer-button-up 272");
    assert_eq!(
        geometry(&window(&compositor, foreground)),
        [500, 500, 240, 160]
    );
    assert_eq!(geometry(&window(&compositor, covered)), original);

    client.sync();
    assert_eq!(
        geometry(&window(&compositor, foreground)),
        [200, 200, 560, 460]
    );
    let revealed = wait_position(&compositor, covered, 200, 176);
    assert_eq!(geometry(&revealed), [200, 176, 240, 160]);
    assert!(!client.configuration(1).1);
    shutdown(compositor);
}

#[test]
fn canvas_peek_committed_resize_settles_without_waiting_for_release_state_ack() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let (mut client, [covered, foreground]) = interactive(&compositor);
    action(&compositor, format!("move-window {foreground} 500 500"));
    let original = geometry(&window(&compositor, covered));
    let point = resize_from_top_left(&compositor, &mut client, foreground);

    resize_motion(&compositor, point, -300.0);
    client.sync();
    assert_eq!(
        geometry(&window(&compositor, foreground)),
        [200, 200, 560, 460]
    );
    assert_eq!(geometry(&window(&compositor, covered)), original);
    action(&compositor, "pointer-button-up 272");

    // The final size is already committed. Withhold the Resizing-unset ack:
    // a pending anchor alone must not freeze this or later canvas settlement.
    let revealed = wait_position(&compositor, covered, 200, 176);
    assert_eq!(geometry(&revealed), [200, 176, 240, 160]);
    client.sync();
    assert_eq!(geometry(&window(&compositor, covered)), geometry(&revealed));
    assert_eq!(
        geometry(&window(&compositor, foreground)),
        [200, 200, 560, 460]
    );
    shutdown(compositor);
}
