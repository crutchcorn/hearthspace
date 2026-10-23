//! Slow native window creation must not lose text or postpone cancellation.
use super::*;

#[test]
fn canvas_search_typing_survives_delayed_field_and_old_session_teardown() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    action(
        &compositor,
        format!(
            "canvas-search-register {}",
            json!({"offset":0,"entries":[
                {"id":"control:sound","title":"Sound","search_text":"volume"}
            ]})
        ),
    );
    action(&compositor, "canvas-search-open");
    let first = search(&compositor);
    let old_field = field(&mut client, &first);
    let old_card = client.add(
        hearthspace_common::canvas_search_result_app_id(
            first["session"].as_u64().unwrap(),
            "control:sound",
        ),
        (240, 72),
        (0, 0),
        GREEN,
    );
    action(
        &compositor,
        format!("canvas-search-close {}", first["session"]),
    );
    action(&compositor, "canvas-search-open");
    let current = search(&compositor);
    let before = client.keys().len();
    keys(&compositor, &[29, 30]); // Ctrl+A, followed by 'notes'.
    for key in [49, 24, 20, 18, 31] {
        keys(&compositor, &[key]);
    }
    // A normal uppercase query needs extra Shift events. More than the legacy
    // 64-event launcher bound must survive a slow Canvas Search field as well.
    for _ in 0..29 {
        keys(&compositor, &[42, 30]);
    }
    client.destroy(old_field);
    client.destroy(old_card);
    // The launcher bridge's one-second event lifetime is not a typing deadline
    // for an active Canvas Search session whose native field is still loading.
    thread::sleep(Duration::from_millis(1200));
    let current_field = field(&mut client, &current);
    assert_eq!(client.focus(), Some(current_field));
    let mut expected = vec![29, 30, 30, 29, 49, 49, 24, 24, 20, 20, 18, 18, 31, 31];
    expected.extend([42, 30, 30, 42].repeat(29));
    assert_eq!(&client.keys()[before..], expected);
    assert!(
        client.enter_keys().is_empty(),
        "replayed modifier pairs must finish cleanly"
    );
    assert_eq!(search(&compositor)["session"], current["session"]);
}

#[test]
fn canvas_search_escape_cancels_before_the_native_field_maps() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    action(&compositor, "canvas-search-open");
    let pending = search(&compositor);
    keys(&compositor, &[49]);
    keys(&compositor, &[1]);
    assert_eq!(search(&compositor)["active"], false);
    let late_field = field(&mut client, &pending);
    assert!(client.closed().contains(&late_field));
    assert!(
        client.keys().is_empty(),
        "cancelled typing must not reach a late surface"
    );
}
