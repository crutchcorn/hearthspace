//! Separate menu surfaces share input ownership without changing the parent.
use super::*;

fn map_submenu(client: &mut MapClient, menu: &Value, row_y: u32) -> usize {
    client.add(
        desktop_context::submenu_app_id(
            menu["generation"].as_u64().unwrap(),
            menu["output"].as_str(),
            row_y,
        ),
        (160, 112),
        (12, 10),
        [255, 255, 0, 255],
    )
}

fn panels(compositor: &HeadlessCompositor) -> Vec<Value> {
    query(compositor, "status")["shell_panels"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|panel| panel["kind"] == "desktop-context")
        .cloned()
        .collect()
}

fn child_panel(compositor: &HeadlessCompositor, parent: &Value) -> Value {
    let panels = panels(compositor);
    assert_eq!(panels.len(), 2, "both menu surfaces must remain mapped");
    assert!(
        panels.contains(parent),
        "parent geometry must stay unchanged"
    );
    panels
        .into_iter()
        .find(|panel| panel["width"] == 160 && panel["height"] == 112)
        .expect("visible child menu")
}

fn click_panel(
    compositor: &HeadlessCompositor,
    client: &mut MapClient,
    surface: usize,
    bounds: &Value,
    inset: (f64, f64),
) {
    let presses = client.presses().len();
    let releases = client.releases().len();
    click(
        compositor,
        bounds["x"].as_f64().unwrap() + 20.0,
        bounds["y"].as_f64().unwrap() + 20.0,
    );
    let event = (surface, (20.0 + inset.0, 20.0 + inset.1));
    assert_eq!(&client.presses()[presses..], &[event]);
    assert_eq!(&client.releases()[releases..], &[event]);
    assert_eq!(client.focus(), Some(surface));
}

#[test]
fn desktop_submenu_keeps_parent_geometry_and_both_surfaces_clickable() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, normal, _) = fixture();
    right_click(&compositor, point(&compositor, "file:61"));
    let menu = context(&compositor);
    let parent = map_menu(&mut client, &menu);
    let before = panel(&compositor);
    let child = map_submenu(&mut client, &menu, 68);
    let child_bounds = child_panel(&compositor, &before);
    assert_eq!(client.focus(), Some(child));
    assert_eq!(client.size(parent), (210, 170));
    assert_eq!(child_bounds["output"], menu["output"]);
    assert_eq!(
        child_bounds["x"].as_i64().unwrap(),
        before["x"].as_i64().unwrap() + 210
    );
    assert_eq!(
        child_bounds["y"].as_i64().unwrap(),
        before["y"].as_i64().unwrap() + 68
    );
    assert!(
        client.closed().is_empty(),
        "mapping the child must not dismiss its parent"
    );

    click_panel(&compositor, &mut client, child, &child_bounds, (12.0, 10.0));
    click_panel(&compositor, &mut client, parent, &before, (8.0, 6.0));
    action(
        &compositor,
        format!("desktop-context-child {}", menu["generation"]),
    );
    assert_eq!(
        client.focus(),
        Some(child),
        "Right reenters an existing child"
    );
    click_panel(&compositor, &mut client, child, &child_bounds, (12.0, 10.0));
    assert_eq!(context(&compositor), menu);
    assert_eq!(child_panel(&compositor, &before), child_bounds);
    assert!(
        client.closed().is_empty(),
        "presses and focus changes within the menu family must not dismiss either surface"
    );

    click(&compositor, 650.0, 330.0);
    assert!(context(&compositor).is_null());
    assert!(client.closed().contains(&parent));
    assert!(client.closed().contains(&child));
    assert_eq!(client.presses().last().unwrap().0, normal);
    assert_eq!(client.releases().last().unwrap().0, normal);
    assert_eq!(client.focus(), Some(normal));
    client.destroy(child);
    client.destroy(parent);
    assert_eq!(client.focus(), Some(normal));
    finish(&mut compositor);
}

#[test]
fn desktop_submenu_flips_left_and_clamps_with_logical_output_coordinates() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("1280x800", None, None, 2);
    drop(compositor.wait_for_socket());
    let mut client = connect(&compositor);
    right_click(&compositor, (638.0, 100.0));
    let menu = context(&compositor);
    let parent = map_menu(&mut client, &menu);
    let before = panel(&compositor);
    assert_eq!(
        (before["x"].as_i64(), before["y"].as_i64()),
        (Some(422), Some(100))
    );
    let child = map_submenu(&mut client, &menu, 68);
    let child_bounds = child_panel(&compositor, &before);
    assert_eq!(
        (child_bounds["x"].as_i64(), child_bounds["y"].as_i64()),
        (Some(262), Some(168)),
        "child touches the parent's left edge and keeps the row offset at scale 2"
    );
    click_panel(&compositor, &mut client, child, &child_bounds, (12.0, 10.0));
    click_panel(&compositor, &mut client, parent, &before, (8.0, 6.0));
    assert!(client.closed().is_empty());
    action(&compositor, "pan-to -230 170");
    action(&compositor, "zoom-out");
    assert_eq!(child_panel(&compositor, &before), child_bounds);
    close(&compositor, &menu);
    client.destroy(child);
    client.destroy(parent);

    right_click(&compositor, (638.0, 398.0));
    let menu = context(&compositor);
    let parent = map_menu(&mut client, &menu);
    let before = panel(&compositor);
    assert_eq!(
        (before["x"].as_i64(), before["y"].as_i64()),
        (Some(422), Some(222))
    );
    let child = map_submenu(&mut client, &menu, 68);
    let child_bounds = child_panel(&compositor, &before);
    assert_eq!(
        (child_bounds["x"].as_i64(), child_bounds["y"].as_i64()),
        (Some(262), Some(280)),
        "child bottom keeps an 8px logical output margin when the row would overflow"
    );
    click_panel(&compositor, &mut client, child, &child_bounds, (12.0, 10.0));
    assert_eq!(context(&compositor), menu);
    assert!(!client.closed().contains(&parent));
    assert!(!client.closed().contains(&child));
    close(&compositor, &menu);
    client.destroy(child);
    client.destroy(parent);
    finish(&mut compositor);
}

#[test]
fn desktop_submenu_back_restores_parent_focus_on_either_side_of_child_teardown() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, items) = fixture();
    right_click(&compositor, point(&compositor, "file:61"));
    let menu = context(&compositor);
    let parent = map_menu(&mut client, &menu);
    let before = panel(&compositor);
    let child = map_submenu(&mut client, &menu, 68);
    assert_eq!(client.focus(), Some(child));

    let back = format!("desktop-context-parent {}", menu["generation"]);
    action(&compositor, &back);
    assert_eq!(client.focus(), Some(parent));
    client.destroy(child);
    assert_eq!(
        client.focus(),
        Some(parent),
        "late child teardown retains parent focus"
    );
    assert_eq!(context(&compositor), menu);
    assert_eq!(panels(&compositor), vec![before.clone()]);
    assert!(!client.closed().contains(&parent));

    // Window destruction and the shell's control command travel on separate
    // connections, so also cover destruction arriving before the Back command.
    let child = map_submenu(&mut client, &menu, 68);
    assert_eq!(client.focus(), Some(child));
    client.destroy(child);
    action(&compositor, &back);
    assert_eq!(client.focus(), Some(parent));
    assert_eq!(context(&compositor), menu);
    assert_eq!(panels(&compositor), vec![before]);
    assert!(
        !client.closed().contains(&parent),
        "child teardown must not dismiss the parent before its focus command arrives"
    );
    close(&compositor, &menu);
    client.destroy(parent);
    assert_eq!(client.focus(), Some(items[0]));
    finish(&mut compositor);
}

#[test]
fn desktop_submenu_rejects_late_maps_and_back_commands_after_close_or_replacement() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, _, items) = fixture();
    right_click(&compositor, point(&compositor, "file:61"));
    let first = context(&compositor);
    let parent = map_menu(&mut client, &first);
    close(&compositor, &first);
    client.destroy(parent);
    let late = map_submenu(&mut client, &first, 68);
    assert!(client.closed().contains(&late));
    assert_eq!(client.focus(), Some(items[0]));
    assert!(panels(&compositor).is_empty());
    assert!(context(&compositor).is_null());
    assert!(
        compositor
            .send_text_command(&format!("desktop-context-parent {}", first["generation"]))
            .starts_with("err ")
    );
    client.destroy(late);

    right_click(&compositor, point(&compositor, "file:62"));
    let current = context(&compositor);
    assert_ne!(current["generation"], first["generation"]);
    let parent = map_menu(&mut client, &current);
    let before = panel(&compositor);
    let child = map_submenu(&mut client, &current, 68);
    let child_bounds = child_panel(&compositor, &before);
    assert_eq!(client.focus(), Some(child));

    let stale = map_submenu(&mut client, &first, 68);
    assert!(client.closed().contains(&stale));
    assert_eq!(
        client.focus(),
        Some(child),
        "stale child must not steal focus"
    );
    assert!(
        compositor
            .send_text_command(&format!("desktop-context-parent {}", first["generation"]))
            .starts_with("err ")
    );
    close(&compositor, &first);
    client.destroy(stale);
    assert_eq!(client.focus(), Some(child));
    assert_eq!(context(&compositor), current);
    assert_eq!(child_panel(&compositor, &before), child_bounds);
    assert!(!client.closed().contains(&parent));
    assert!(!client.closed().contains(&child));
    close(&compositor, &current);
    assert!(client.closed().contains(&parent));
    assert!(client.closed().contains(&child));
    client.destroy(child);
    client.destroy(parent);
    assert_eq!(client.focus(), Some(items[1]));
    finish(&mut compositor);
}
