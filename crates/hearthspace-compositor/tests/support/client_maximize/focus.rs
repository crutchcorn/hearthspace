//! Revealing a pending focused view must not pan before activation promotes it.
use super::*;
use std::time::{Duration, Instant};

fn alt_tab(compositor: &HeadlessCompositor, target: u64) {
    let before = query(compositor, "status")["viewport"].clone();
    action(compositor, "key-down 56");
    keys(compositor, &[15]);
    let preview = query(compositor, "status");
    assert_eq!(preview["window_switcher"]["selected_window_id"], target);
    assert_eq!(preview["viewport"], before);
    action(compositor, "key-up 56");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = query(compositor, "status");
        assert_eq!(
            status["viewport"], before,
            "Alt-Tab panned during activation"
        );
        if status["window_switcher"]["animating"] == false {
            break;
        }
        assert!(Instant::now() < deadline, "switcher exit never settled");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(window(compositor, target)["focused"], true);
}

fn pending_ancestor_fixture() -> (HeadlessCompositor, InteractiveClient, u64, u64) {
    let (compositor, mut client, [a, parent]) = fixture();
    let dialog = add_unmapped(&mut client, Some(1), false);
    map(&mut client, dialog);
    // Use a grandchild so the reveal predicate must follow the same bounded
    // ancestor chain as the actual focus callback, beyond the immediate parent.
    let nested = add_unmapped(&mut client, Some(dialog), false);
    map(&mut client, nested);
    let target = deferred_id(&compositor, nested);
    action(&compositor, format!("move-window {target} 9000 -7000"));
    action(&compositor, format!("focus-window {a}"));
    client.gtk_request_maximized(1, true);
    action(&compositor, "pan-to -4000 3000");
    assert_eq!(window(&compositor, target)["visible"], false);
    assert_eq!(window(&compositor, target)["maximized"], false);
    assert_eq!(window(&compositor, parent)["focused_view"], false);
    (compositor, client, parent, target)
}

#[test]
fn alt_tab_promotes_offscreen_pending_window_without_panning() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [_, b]) = fixture();
    client.gtk_request_maximized(1, true);
    action(&compositor, "pan-to -4000 3000");
    assert_eq!(window(&compositor, b)["visible"], false);
    let before = query(&compositor, "status")["viewport"].clone();
    alt_tab(&compositor, b);
    assert_focused(&compositor, &mut client, b);
    assert_eq!(query(&compositor, "status")["viewport"], before);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn explicit_dialog_focus_promotes_pending_ancestor_without_panning() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, parent, target) = pending_ancestor_fixture();
    let before = query(&compositor, "status")["viewport"].clone();
    action(&compositor, format!("focus-window {target}"));
    assert_eq!(query(&compositor, "status")["viewport"], before);
    assert_eq!(window(&compositor, target)["focused"], true);
    assert_focused(&compositor, &mut client, parent);
    assert_eq!(window(&compositor, parent)["focused"], false);
    assert_eq!(window(&compositor, target)["output_coordinates"], true);
    assert_eq!(query(&compositor, "status")["viewport"], before);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn alt_tab_dialog_focus_promotes_pending_ancestor_without_panning() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, parent, target) = pending_ancestor_fixture();
    let before = query(&compositor, "status")["viewport"].clone();
    alt_tab(&compositor, target);
    assert_focused(&compositor, &mut client, parent);
    assert_eq!(window(&compositor, parent)["focused"], false);
    assert_eq!(window(&compositor, target)["focused"], true);
    assert_eq!(window(&compositor, target)["output_coordinates"], true);
    assert_eq!(query(&compositor, "status")["viewport"], before);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
