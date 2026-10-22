use super::*;

#[test]
fn canvas_peek_waits_for_release_exposes_nearest_header_and_keeps_window_position() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let (mut client, [covered, foreground]) = interactive(&compositor);
    let original = window(&compositor, covered);
    assert_eq!(original["titlebar_height"], 44);

    drag_header_to(&compositor, foreground, (200, 200));
    client.sync();
    assert_eq!(
        geometry(&window(&compositor, covered)),
        geometry(&original),
        "a fully covered window must remain still while the pointer is held"
    );
    assert_eq!(window(&compositor, foreground)["canvas_x"], 200);
    assert_eq!(window(&compositor, foreground)["canvas_y"], 200);
    action(&compositor, "pointer-button-up 272");

    // Identical outer rectangles need exactly 24px. Equal distances prefer up.
    let revealed = wait_position(&compositor, covered, 200, 176);
    assert_eq!(
        geometry(&window(&compositor, foreground)),
        [200, 200, 240, 160],
        "the window the user dropped must retain its position and size"
    );
    assert_eq!(revealed["content_width"], original["content_width"]);
    assert_eq!(revealed["content_height"], original["content_height"]);
    assert_eq!(window(&compositor, foreground)["focused"], true);

    // This point is inside the revealed SSD header, above the covering window.
    click(&compositor, 240.0, 188.0);
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|w| w["id"] == covered && w["focused"] == true)
    });
    assert!(
        !client.received_pointer_press(),
        "exposed chrome must own its click"
    );
    assert!(client.released_buttons().is_empty());
    action(&compositor, format!("move-window {foreground} 800 200"));
    client.sync();
    assert_eq!(
        geometry(&window(&compositor, covered)),
        geometry(&revealed),
        "windows keep their peek position after the obstacle leaves"
    );
    shutdown(compositor);
}

#[test]
fn canvas_peek_translates_maximized_bounds_and_saved_restore_position_together() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let (mut client, [maximized, foreground]) = interactive(&compositor);
    action(&compositor, format!("move-window {foreground} 3000 200"));
    action(&compositor, format!("move-window {maximized} 200 12"));
    let original = geometry(&window(&compositor, maximized));
    action(&compositor, "pan-to 0 0");
    action(&compositor, format!("maximize-window {maximized} true"));
    client.sync();
    let before = window(&compositor, maximized);
    assert_eq!(
        before["maximized_canvas_bounds"],
        json!({"x":24,"y":96,"width":1232,"height":608})
    );
    assert_eq!(before["focused_view"], false);

    // Maximizing the second app completely covers the first, without moving
    // its saved floating position onto the canvas before the intended trigger.
    action(&compositor, format!("maximize-window {foreground} true"));
    client.sync();
    let peeked = wait_position(&compositor, maximized, 200, -12);
    assert_eq!(
        peeked["maximized_canvas_bounds"],
        json!({"x":24,"y":72,"width":1232,"height":608})
    );
    assert_eq!(peeked["maximized"], true);
    assert_eq!(peeked["content_width"], before["content_width"]);
    assert_eq!(peeked["content_height"], before["content_height"]);
    assert_eq!(
        window(&compositor, foreground)["maximized_canvas_bounds"],
        before["maximized_canvas_bounds"]
    );

    client.unmap(1);
    wait_windows(&compositor, |windows| windows.len() == 1);
    action(&compositor, format!("restore-window {maximized}"));
    client.sync();
    let restored = window(&compositor, maximized);
    assert_eq!(
        geometry(&restored),
        [original[0], original[1] - 24, original[2], original[3]],
        "restore must retain the translation, including a negative canvas home"
    );
    assert_eq!(restored["maximized"], false);
    shutdown(compositor);
}
