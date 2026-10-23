//! Maximization must commit before its new origin can displace neighbors.
use super::*;

fn resize_client(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    number: usize,
    id: u64,
    size: (i32, i32),
) {
    // Fixed client limits let a small pointer motion establish either size,
    // including a neighbor shorter than the default SSD resize minimum.
    client.set_size_limits(number, size, size);
    let before = window(compositor, id);
    let x = before["x"].as_f64().unwrap() + 120.0;
    let y = before["y"].as_f64().unwrap() + 100.0;
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 272");
    let serial = client.press_serial();
    client.request_resize(number, serial);
    action(
        compositor,
        format!("pointer-motion-abs {} {}", x + 1.0, y + 1.0),
    );
    action(compositor, "pointer-button-up 272");
    client.sync();
    client.set_size_limits(number, (0, 0), (0, 0));
    let resized = window(compositor, id);
    assert_eq!(resized["content_width"], size.0);
    assert_eq!(resized["content_height"], size.1);
}

#[test]
fn canvas_peek_maximize_waits_for_commit_before_settling_new_origin() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let (mut client, [covered, foreground]) = interactive(&compositor);
    resize_client(&compositor, &mut client, 0, covered, (260, 116));
    resize_client(&compositor, &mut client, 1, foreground, (600, 356));
    action(&compositor, format!("move-window {foreground} 3000 200"));
    action(&compositor, format!("move-window {covered} 364 296"));
    action(&compositor, "pan-to 0 0");
    let original = geometry(&window(&compositor, covered));
    assert_eq!(original, [364, 296, 260, 116]);
    let viewport = query(&compositor, "status")["viewport"].clone();

    action(&compositor, format!("maximize-window {foreground} true"));
    // Withhold sync: the new origin (24,96) still carries the old 600x400
    // outer rectangle. Settling now would push the neighbor right by 24px.
    let pending = window(&compositor, foreground);
    assert_eq!(pending["maximized"], true);
    assert_eq!(pending["focused_view"], false);
    assert_eq!(
        ["x", "y", "width", "height"].map(|field| pending[field].as_i64().unwrap()),
        [24, 96, 600, 400]
    );
    assert_eq!(
        geometry(&window(&compositor, covered)),
        original,
        "the provisional maximized rectangle must not move its neighbor"
    );

    client.sync();
    let committed = window(&compositor, foreground);
    assert_eq!(
        ["x", "y", "width", "height"].map(|field| committed[field].as_i64().unwrap()),
        [24, 96, 1232, 608]
    );
    // Only the committed rectangle participates: up is now the shortest move,
    // and x must retain its original value rather than a provisional rightward shift.
    let revealed = wait_position(&compositor, covered, 364, 72);
    assert_eq!(geometry(&revealed), [364, 72, 260, 116]);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    shutdown(compositor);
}

#[test]
fn canvas_peek_same_size_maximize_waits_for_committed_maximized_state() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let (mut client, [covered, foreground]) = interactive(&compositor);
    resize_client(&compositor, &mut client, 1, foreground, (1232, 564));
    action(&compositor, format!("move-window {foreground} 3000 200"));
    action(&compositor, format!("move-window {covered} 364 296"));
    action(&compositor, "pan-to 0 0");
    let original = geometry(&window(&compositor, covered));

    // Both configures specify 1232x564. Only the Maximized flag distinguishes
    // the committed floating buffer from the pending presentation transaction.
    action(&compositor, format!("maximize-window {foreground} true"));
    assert_eq!(
        geometry(&window(&compositor, covered)),
        original,
        "an unchanged configured size does not commit maximization"
    );
    client.sync();
    let revealed = wait_position(&compositor, covered, 364, 72);
    assert_eq!(geometry(&revealed), [364, 72, 240, 160]);
    shutdown(compositor);
}
