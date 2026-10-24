//! Real transient families retain identity when their stack positions change.
use super::*;
use crate::interactive::client::events::{assert_content_click, assert_reentered, motion};

fn click_overlap(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
    number: usize,
    id: u64,
) {
    motion(compositor, (1100.0, 700.0));
    client.take_pointer_events();
    motion(compositor, (240.0, 240.0));
    let events = client.take_pointer_events();
    assert_reentered(client, &events, number);
    assert_content_click(compositor, client);
    assert_eq!(window(compositor, id)["focused"], true);
}

#[test]
fn transient_family_restack_preserves_targets_across_remap_reclassification_and_destroy() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [parent, unrelated]) = fixture();
    let child = add_unmapped(&mut client, Some(0), false);
    map(&mut client, child);
    let child_id = deferred_id(&compositor, child);
    let grandchild = add_unmapped(&mut client, Some(child), false);
    map(&mut client, grandchild);
    let grandchild_id = deferred_id(&compositor, grandchild);
    let sibling = add_unmapped(&mut client, Some(0), false);
    map(&mut client, sibling);
    let sibling_id = deferred_id(&compositor, sibling);

    // Reclassification moves another record across the shell boundary without
    // replacing it. Raising a nested branch must move its descendants together.
    client.state.windows[1]
        .toplevel
        .set_app_id(hearthspace_common::CONTROL_OSD_APP_ID.into());
    client.sync();
    wait_windows(&compositor, |windows| windows.len() == 4);
    client.state.windows[1]
        .toplevel
        .set_app_id("dev.hearthspace.InteractiveTest".into());
    client.sync();
    wait_windows(&compositor, |windows| windows.len() == 5);
    assert_eq!(
        window(&compositor, unrelated)["title"],
        "Interactive fixture 1"
    );
    for id in [parent, unrelated, child_id, grandchild_id, sibling_id] {
        action(&compositor, format!("move-window {id} 200 200"));
    }
    action(&compositor, format!("focus-window {child_id}"));
    action(&compositor, format!("focus-window {unrelated}"));
    action(&compositor, format!("focus-window {parent}"));
    click_overlap(&compositor, &mut client, grandchild, grandchild_id);

    // Raising the whole family preserves its most recently raised sibling.
    action(&compositor, format!("focus-window {sibling_id}"));
    action(&compositor, format!("focus-window {unrelated}"));
    action(&compositor, format!("focus-window {parent}"));
    click_overlap(&compositor, &mut client, sibling, sibling_id);
    let viewport = query(&compositor, "status")["viewport"].clone();
    client.unmap(sibling);
    wait_windows(&compositor, |windows| windows.len() == 4);
    assert_eq!(window(&compositor, grandchild_id)["focused"], true);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);

    // XDG unmap resets role properties. Reapply them on the same protocol
    // objects, proving that remap retains the existing compositor identity.
    let toplevel = &client.state.windows[sibling].toplevel;
    toplevel.set_app_id("dev.hearthspace.DeferredMaximizeTest".into());
    toplevel.set_title(format!("Deferred maximize {sibling}"));
    toplevel.set_parent(Some(&client.state.windows[0].toplevel));
    map(&mut client, sibling);
    assert_eq!(deferred_id(&compositor, sibling), sibling_id);
    action(&compositor, format!("move-window {sibling_id} 200 200"));
    action(&compositor, format!("focus-window {unrelated}"));
    action(&compositor, format!("focus-window {parent}"));
    click_overlap(&compositor, &mut client, sibling, sibling_id);

    client.destroy(sibling);
    wait_windows(&compositor, |windows| windows.len() == 4);
    assert_eq!(window(&compositor, grandchild_id)["focused"], true);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    click_overlap(&compositor, &mut client, grandchild, grandchild_id);

    let replacement = add_unmapped(&mut client, Some(0), false);
    map(&mut client, replacement);
    let replacement_id = deferred_id(&compositor, replacement);
    assert!(
        replacement_id > sibling_id,
        "retired IDs must never be reused"
    );
    assert!(
        compositor
            .send_text_command(&format!("focus-window {sibling_id}"))
            .starts_with("err unknown window")
    );
    action(&compositor, format!("move-window {replacement_id} 200 200"));
    action(&compositor, format!("focus-window {unrelated}"));
    action(&compositor, format!("focus-window {parent}"));
    click_overlap(&compositor, &mut client, replacement, replacement_id);
    assert_eq!(deferred_id(&compositor, child), child_id);
    assert_eq!(deferred_id(&compositor, grandchild), grandchild_id);
    assert_eq!(
        window(&compositor, parent)["title"],
        "Interactive fixture 0"
    );
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
