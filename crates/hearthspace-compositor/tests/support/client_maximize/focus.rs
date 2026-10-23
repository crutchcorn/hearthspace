//! Focused presentation preserves viewport and transient input ownership.
use super::super::events::{assert_content_click, assert_leave, assert_silent};
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

#[test]
fn dialog_touch_drag_survives_parent_maximize_and_restore() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [parent, _]) = fixture();
    let dialog = add_unmapped(&mut client, Some(0), false);
    map(&mut client, dialog);
    let id = deferred_id(&compositor, dialog);
    let original = window(&compositor, id);
    assert_eq!(original["focused"], true);
    assert_eq!(original["maximized"], false);
    assert_eq!(original["fullscreen"], false);
    assert_eq!(original["output_coordinates"], false);
    let position = || {
        let dialog = window(&compositor, id);
        ["canvas_x", "canvas_y"].map(|field| dialog[field].as_i64().unwrap())
    };
    let before = position();
    let p = (
        original["x"].as_f64().unwrap() + 40.0,
        original["y"].as_f64().unwrap() + 30.0,
    );
    let motion = |dx: f64, dy: f64| {
        action(
            &compositor,
            format!("touch-motion finger 0 {} {}", p.0 + dx, p.1 + dy),
        );
        action(&compositor, "touch-frame finger");
    };
    // Start while floating: output-coordinate dialogs cannot start a move.
    // Match the real touch fixture's delayed client takeover after recognition.
    action(&compositor, format!("touch-down finger 0 {} {}", p.0, p.1));
    action(&compositor, "touch-frame finger");
    motion(16.0, 8.0);
    let serial = client.press_serial();
    client.take_pointer_events();
    client.request_move(dialog, serial);
    assert_leave(&mut client, dialog);
    assert_eq!(position(), [before[0] + 16, before[1] + 8]);

    // The focused dialog makes its parent own presentation despite the parent
    // not owning keyboard focus. Do not lift or issue another move request.
    client.gtk_request_maximized(0, true);
    let maximized = window(&compositor, parent);
    assert_eq!(maximized["maximized"], true);
    assert_eq!(maximized["focused_view"], true);
    assert_eq!(maximized["fullscreen"], false);
    assert_eq!(maximized["focused"], false);
    assert_eq!(window(&compositor, id)["focused"], true);
    assert_eq!(window(&compositor, id)["output_coordinates"], true);
    assert_silent(
        &mut client,
        "parent maximization released the dialog's grab",
    );
    motion(40.0, 20.0);
    assert_eq!(
        position(),
        [before[0] + 40, before[1] + 20],
        "parent maximization must retain the original finger's move"
    );

    // Restore the focused-view-owning parent while that same contact is down.
    // The dialog is centered while pinned; after restore its motion is visible.
    client.request_restore(0);
    let restored = window(&compositor, parent);
    for field in ["maximized", "focused_view", "fullscreen", "focused"] {
        assert_eq!(restored[field], false, "parent {field}");
    }
    assert_eq!(window(&compositor, id)["focused"], true);
    assert_eq!(window(&compositor, id)["output_coordinates"], false);
    assert_silent(&mut client, "parent restore released the dialog's grab");
    motion(80.0, 40.0);
    let completed = [before[0] + 80, before[1] + 40];
    assert_eq!(
        position(),
        completed,
        "parent restore must retain the original finger's move"
    );
    let moved = window(&compositor, id);
    assert_eq!(
        moved["x"].as_i64().unwrap(),
        original["x"].as_i64().unwrap() + 80
    );
    assert_eq!(
        moved["y"].as_i64().unwrap(),
        original["y"].as_i64().unwrap() + 40
    );

    action(&compositor, "touch-up finger 0");
    action(&compositor, "touch-frame finger");
    action(
        &compositor,
        format!(
            "pointer-motion-abs {} {}",
            moved["x"].as_i64().unwrap() + 60,
            moved["y"].as_i64().unwrap() + 60
        ),
    );
    client.take_pointer_events();
    assert_content_click(&compositor, &mut client);
    assert_eq!(position(), completed, "lift must finish the dialog move");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
