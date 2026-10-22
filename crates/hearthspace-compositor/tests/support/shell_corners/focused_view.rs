use super::*;
use crate::navigation::keys;

fn visible_corners(compositor: &HeadlessCompositor) -> usize {
    query(compositor, "status")["shell_corners"]
        .as_array()
        .unwrap()
        .len()
}

#[test]
fn focused_view_reveals_corners_on_hover_and_keyboard_without_exiting() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.FocusedCornerTest".into(),
        (240, 160),
        (0, 0),
        COLOR,
    );
    for corner in [
        ShellCorner::Notifications,
        ShellCorner::System,
        ShellCorner::Canvas,
        ShellCorner::Apps,
    ] {
        client.add(shell_corner_app_id(corner, None), (48, 54), (0, 0), COLOR);
    }
    action(&compositor, "pointer-motion-abs 600 400");
    keys(&compositor, &[125, 42, 33]); // Super+Shift+F: focused view.
    client.sync();
    assert_eq!(
        query(&compositor, "windows")["windows"][0]["focused_view"],
        true
    );
    assert_eq!(visible_corners(&compositor), 0);
    action(&compositor, "pointer-motion-abs 5 5");
    assert_eq!(visible_corners(&compositor), 4);
    action(&compositor, "pointer-motion-abs 600 400");
    assert_eq!(
        visible_corners(&compositor),
        4,
        "hover grace prevents immediate dismissal"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while visible_corners(&compositor) != 0 {
        assert!(
            Instant::now() < deadline,
            "shell did not tuck after hover grace"
        );
        thread::sleep(Duration::from_millis(20));
    }
    keys(&compositor, &[1]); // Escape belongs to the app, not focused view.
    assert_eq!(
        query(&compositor, "windows")["windows"][0]["focused_view"],
        true
    );
    keys(&compositor, &[125, 64]); // Super+F6 exposes and focuses the shell.
    assert_ne!(client.focus(), Some(normal));
    assert_eq!(visible_corners(&compositor), 4);
    keys(&compositor, &[1]);
    assert_eq!(client.focus(), Some(normal));
    assert_eq!(
        query(&compositor, "windows")["windows"][0]["focused_view"],
        true
    );
    keys(&compositor, &[125, 33]); // Actual fullscreen has no corner reveal.
    client.sync();
    action(&compositor, "pointer-motion-abs 5 5");
    assert_eq!(visible_corners(&compositor), 0);
    keys(&compositor, &[125, 33]);
    client.sync();
    assert_eq!(
        query(&compositor, "windows")["windows"][0]["focused_view"],
        true
    );
    keys(&compositor, &[125, 42, 33]);
    client.sync();
    let window = query(&compositor, "windows")["windows"][0].clone();
    assert_eq!(window["focused_view"], false);
    assert_eq!(window["maximized"], true);
    assert_eq!(window["x"], 24);
    assert_eq!(window["y"], 96);
    assert_eq!(visible_corners(&compositor), 4);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
