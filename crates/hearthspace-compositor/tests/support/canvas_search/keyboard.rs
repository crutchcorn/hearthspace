//! Tool chords cross native field mapping without losing input ownership.
use super::*;

#[test]
fn meta_search_opens_immediately_preserves_query_and_leaves_on_another_tool_chord() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (app, _) = normal(&mut client, &compositor, "dev.hearthspace.MetaSearch");
    action(&compositor, "key-down 125");
    action(&compositor, "key-down 37");
    action(&compositor, "key-down 37");
    let opened = search(&compositor);
    assert_eq!(opened["active"], true);
    assert_eq!(query(&compositor, "layout-status")["canvas_tool"], "search");
    action(&compositor, "key-up 125");
    action(&compositor, "key-up 37");
    keys(&compositor, &[35]);
    let native_field = field(&mut client, &opened);
    assert_eq!(client.focus(), Some(native_field));
    assert!(
        !client.keys().contains(&37),
        "opening K leaked into the field"
    );
    assert_eq!(client.keys().iter().filter(|key| **key == 35).count(), 2);
    assert!(!client.enter_keys().contains(&37));
    update(&compositor, opened["session"].as_u64().unwrap(), "notes");
    keys(&compositor, &[125, 37]);
    assert_eq!(search(&compositor)["session"], opened["session"]);
    assert_eq!(search(&compositor)["query"], "notes");
    let before = client.keys().len();
    action(&compositor, "key-down 125");
    action(&compositor, "key-down 47");
    assert_eq!(search(&compositor)["active"], false);
    assert_eq!(query(&compositor, "layout-status")["canvas_tool"], "select");
    assert!(client.closed().contains(&native_field));
    assert_eq!(client.focus(), Some(app));
    action(&compositor, "key-up 125");
    action(&compositor, "key-up 47");
    assert!(!client.keys()[before..].contains(&47));
    keys(&compositor, &[47]);
    assert_eq!(client.keys().iter().filter(|key| **key == 47).count(), 2);
}

#[test]
fn meta_select_cancels_a_pending_search_and_never_replays_discarded_typing() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    let (app, _) = normal(&mut client, &compositor, "dev.hearthspace.PendingTool");
    keys(&compositor, &[125, 37]);
    let pending = search(&compositor);
    keys(&compositor, &[42, 35]);
    action(&compositor, "key-down 125");
    action(&compositor, "key-down 47");
    assert_eq!(search(&compositor)["active"], false);
    assert_eq!(query(&compositor, "layout-status")["canvas_tool"], "select");
    assert_eq!(client.focus(), Some(app));
    action(&compositor, "key-up 125");
    action(&compositor, "key-up 47");
    let late = field(&mut client, &pending);
    assert!(client.closed().contains(&late));
    assert!(!client.keys().contains(&35));
    assert!(!client.keys().contains(&47));
    keys(&compositor, &[48]);
    assert_eq!(client.keys().iter().filter(|key| **key == 48).count(), 2);
}

#[test]
fn canvas_chords_do_not_disrupt_a_search_fields_client_pointer_grab() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    normal(&mut client, &compositor, "dev.hearthspace.SearchGrab");
    keys(&compositor, &[125, 37]);
    let opened = search(&compositor);
    let native_field = field(&mut client, &opened);
    action(
        &compositor,
        format!(
            "pointer-motion-abs {} {}",
            opened["field"]["x"].as_f64().unwrap() + 20.,
            opened["field"]["y"].as_f64().unwrap() + 20.,
        ),
    );
    action(&compositor, "pointer-button-down 272");
    assert_eq!(client.presses().last().unwrap().0, native_field);
    keys(&compositor, &[125, 35]);
    assert_eq!(search(&compositor)["active"], true);
    assert_eq!(search(&compositor)["session"], opened["session"]);
    assert_eq!(query(&compositor, "layout-status")["canvas_tool"], "search");
    assert!(!client.closed().contains(&native_field));
    action(&compositor, "pointer-button-up 272");
    assert_eq!(client.releases().last().unwrap().0, native_field);
    keys(&compositor, &[125, 35]);
    assert_eq!(search(&compositor)["active"], false);
    assert_eq!(query(&compositor, "layout-status")["canvas_tool"], "pan");
}

#[test]
fn canvas_chords_do_not_disrupt_a_search_fields_client_touch_grab() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    client.enable_touch();
    normal(&mut client, &compositor, "dev.hearthspace.SearchTouchGrab");
    keys(&compositor, &[125, 37]);
    let opened = search(&compositor);
    let native_field = field(&mut client, &opened);
    action(
        &compositor,
        format!(
            "touch-down search-field 0 {} {}",
            opened["field"]["x"].as_f64().unwrap() + 20.,
            opened["field"]["y"].as_f64().unwrap() + 20.,
        ),
    );
    action(&compositor, "touch-frame search-field");
    assert_eq!(client.touches().last().unwrap().0, native_field);
    let before = client.keys().len();
    action(&compositor, "key-down 125");
    action(&compositor, "key-down 35");
    action(&compositor, "key-down 35");
    assert_eq!(search(&compositor)["active"], true);
    assert_eq!(search(&compositor)["session"], opened["session"]);
    assert_eq!(query(&compositor, "layout-status")["canvas_tool"], "search");
    assert!(!client.closed().contains(&native_field));
    assert_eq!(client.touch_cancellations(), 0);
    assert!(client.touch_releases().is_empty());
    assert_eq!(&client.keys()[before..], &[125, 35]);
    action(&compositor, "touch-up search-field 0");
    action(&compositor, "touch-frame search-field");
    assert_eq!(client.touch_releases(), &[0]);
    // Admission changed, but the client's held H still owns its release.
    action(&compositor, "key-up 125");
    action(&compositor, "key-up 35");
    assert_eq!(&client.keys()[before..], &[125, 35, 125, 35]);
    keys(&compositor, &[125, 35]);
    assert_eq!(search(&compositor)["active"], false);
    assert_eq!(query(&compositor, "layout-status")["canvas_tool"], "pan");
    assert_eq!(client.touch_cancellations(), 0);
    assert_eq!(client.touch_releases(), &[0]);
}
