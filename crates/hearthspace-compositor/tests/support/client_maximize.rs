//! Client maximization acquires focused presentation only with actual focus.
use super::{InteractiveClient, Window};
use crate::{
    HeadlessCompositor,
    canvas_zoom::wait_scale,
    headless_test_guard,
    navigation::{action, keys, query, wait_windows},
    shell_panels, wait_for_exit,
};
use serde_json::{Value, json};

#[path = "client_maximize/focus.rs"]
mod focus;

fn fixture() -> (HeadlessCompositor, InteractiveClient, [u64; 2]) {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let client = InteractiveClient::connect(&socket);
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let ids = std::array::from_fn(|number| {
        windows
            .iter()
            .find(|window| window["title"] == format!("Interactive fixture {number}"))
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    });
    action(&compositor, format!("move-window {} 100 150", ids[0]));
    action(&compositor, format!("move-window {} 700 150", ids[1]));
    action(&compositor, format!("focus-window {}", ids[0]));
    (compositor, client, ids)
}

fn window(compositor: &HeadlessCompositor, id: u64) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap()
        .clone()
}

fn assert_focused(compositor: &HeadlessCompositor, client: &mut InteractiveClient, id: u64) {
    client.sync();
    let focused = window(compositor, id);
    assert_eq!(focused["maximized"], true);
    assert_eq!(focused["focused_view"], true);
    assert_eq!(focused["fullscreen"], false);
    assert_eq!(focused["titlebar_height"], 0);
    assert_eq!(focused["x"], 0);
    assert_eq!(focused["y"], 0);
    assert_eq!(focused["width"], 1280);
    assert_eq!(focused["height"], 800);
    assert!(
        compositor
            .send_text_command("pan-to 10 20")
            .starts_with("err canvas navigation is unavailable")
    );
}

// This child module extends the existing fixture without duplicating protocol
// dispatch: acknowledge the initial configure, but defer the first buffer.
fn add_unmapped(client: &mut InteractiveClient, parent: Option<usize>, maximize: bool) -> usize {
    let handle = client.queue.handle();
    let number = client.state.windows.len();
    let surface = client
        .state
        .compositor
        .as_ref()
        .unwrap()
        .create_surface(&handle, ());
    let xdg_surface = client
        .state
        .wm_base
        .as_ref()
        .unwrap()
        .get_xdg_surface(&surface, &handle, number);
    let toplevel = xdg_surface.get_toplevel(&handle, number);
    toplevel.set_title(format!("Deferred maximize {number}"));
    toplevel.set_app_id("dev.hearthspace.DeferredMaximizeTest".into());
    toplevel.set_parent(parent.map(|parent| &client.state.windows[parent].toplevel));
    if maximize {
        toplevel.set_maximized();
    }
    client.state.windows.push(Window {
        surface: surface.clone(),
        xdg_surface,
        toplevel,
        size: (240, 160),
        inset: (0, 0),
        configure_count: 0,
        resizing: false,
        mapped: false,
    });
    surface.commit();
    client.sync();
    number
}

fn map(client: &mut InteractiveClient, number: usize) {
    client.state.windows[number].mapped = true;
    client.state.commit_buffer(number, &client.queue.handle());
    client.sync();
}

fn deferred_id(compositor: &HeadlessCompositor, number: usize) -> u64 {
    wait_windows(compositor, |windows| {
        windows
            .iter()
            .any(|window| window["title"] == format!("Deferred maximize {number}"))
    })
    .into_iter()
    .find(|window| window["title"] == format!("Deferred maximize {number}"))
    .unwrap()["id"]
        .as_u64()
        .unwrap()
}

#[test]
fn background_maximize_preserves_navigation_selection_and_animation_until_focus() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [a, b]) = fixture();
    let mut desktop = shell_panels::connect(&compositor);
    desktop.add(
        hearthspace_common::desktop_item_app_id("app:maximize-selection.desktop"),
        (100, 108),
        (0, 0),
        [0, 255, 0, 255],
    );
    action(
        &compositor,
        "desktop-select \"app:maximize-selection.desktop\"",
    );
    // Desktop selection owns keyboard focus; group selection starts from the
    // focused application while retaining the desktop's existing selection.
    action(&compositor, format!("focus-window {a}"));
    assert_eq!(window(&compositor, a)["focused"], true);
    action(&compositor, "group-selection-toggle");
    keys(&compositor, &[57]);
    keys(&compositor, &[28]);
    assert_eq!(query(&compositor, "group-status")["selected"], json!([a]));
    action(&compositor, "zoom-out");
    client.gtk_request_maximized(1, true);
    let background = window(&compositor, b);
    assert_eq!(background["maximized"], true);
    assert_eq!(background["focused_view"], false);
    assert_eq!(background["fullscreen"], false);
    assert_eq!(background["focused"], false);
    assert_eq!(window(&compositor, a)["focused"], true);
    assert_eq!(query(&compositor, "group-status")["selected"], json!([a]));
    assert_eq!(
        query(&compositor, "desktop-status")["items"][0]["selected"],
        true
    );
    wait_scale(&compositor, 0.8);
    action(&compositor, "pan-to -60 -150");
    let panned = window(&compositor, b);
    assert_ne!(
        (panned["x"].clone(), panned["y"].clone()),
        (json!(0), json!(0))
    );
    action(&compositor, "zoom-to 0.64");
    wait_scale(&compositor, 0.64);

    // Repeated requests and unrelated focus changes must retain pending intent.
    client.gtk_request_maximized(1, true);
    action(&compositor, format!("focus-window {a}"));
    let before = query(&compositor, "status")["viewport"].clone();
    action(&compositor, format!("focus-window {b}"));
    assert_focused(&compositor, &mut client, b);
    assert_eq!(query(&compositor, "status")["viewport"], before);
    assert_eq!(query(&compositor, "group-status")["selected"], json!([]));
    assert_eq!(
        query(&compositor, "desktop-status")["items"][0]["selected"],
        false
    );
    client.gtk_request_maximized(1, true);
    assert_focused(&compositor, &mut client, b);

    action(&compositor, format!("focus-window {a}"));
    client.gtk_request_maximized(1, true);
    assert_eq!(window(&compositor, b)["focused_view"], false);
    action(&compositor, "pan-to 10 20");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn premap_maximize_waits_for_focus_and_survives_background_mapping() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [a, b]) = fixture();
    action(&compositor, "group-selection-toggle");
    keys(&compositor, &[57]);
    keys(&compositor, &[28]);
    action(&compositor, "zoom-out");
    let number = add_unmapped(&mut client, None, true);
    assert!(client.state.windows[number].configure_count > 0);
    assert_eq!(
        query(&compositor, "windows")["windows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(window(&compositor, a)["focused"], true);
    assert_eq!(query(&compositor, "group-status")["selected"], json!([a]));
    wait_scale(&compositor, 0.8);
    // Selection preservation is established. Release the group's ownership of
    // presses on A so the next press starts an actual client pointer grab.
    action(&compositor, "group-clear");
    assert_eq!(query(&compositor, "group-status")["selected"], json!([]));
    action(&compositor, format!("focus-window {b}"));
    action(&compositor, format!("focus-window {a}"));
    let foreground = window(&compositor, a);
    action(
        &compositor,
        format!(
            "pointer-motion-abs {} {}",
            foreground["x"].as_i64().unwrap() + 30,
            foreground["y"].as_i64().unwrap() + 30
        ),
    );
    action(&compositor, "pointer-button-down 272");
    client.press_serial();
    map(&mut client, number);
    let id = deferred_id(&compositor, number);
    assert_eq!(window(&compositor, a)["focused"], true);
    assert_eq!(window(&compositor, id)["mapped"], true);
    assert_eq!(window(&compositor, id)["focused_view"], false);
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "pan-to -100 -100");
    action(&compositor, format!("focus-window {id}"));
    assert_focused(&compositor, &mut client, id);

    // A startup request also activates automatically when first mapping really
    // grants keyboard focus, without requiring a second maximize request.
    action(&compositor, format!("focus-window {a}"));
    let startup = add_unmapped(&mut client, None, true);
    map(&mut client, startup);
    let startup_id = deferred_id(&compositor, startup);
    assert_eq!(window(&compositor, startup_id)["focused"], true);
    assert_focused(&compositor, &mut client, startup_id);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn client_focused_view_survives_shell_and_transient_focus_but_not_another_application() {
    use hearthspace_common::{ShellCorner, shell_corner_app_id};
    let _guard = headless_test_guard();
    let (mut compositor, mut client, [a, b]) = fixture();
    let mut shell = shell_panels::connect(&compositor);
    for corner in [
        ShellCorner::Notifications,
        ShellCorner::System,
        ShellCorner::Canvas,
        ShellCorner::Apps,
    ] {
        shell.add(
            shell_corner_app_id(corner, None),
            (48, 54),
            (0, 0),
            [0, 255, 0, 255],
        );
    }
    action(&compositor, "pointer-motion-abs 600 400");
    client.gtk_request_maximized(0, true);
    assert_focused(&compositor, &mut client, a);
    assert_eq!(query(&compositor, "status")["shell_corners"], json!([]));
    keys(&compositor, &[125, 64]); // Super+F6 focuses shell chrome.
    assert_eq!(window(&compositor, a)["focused"], false);
    client.gtk_request_maximized(0, true);
    assert_focused(&compositor, &mut client, a);
    assert_eq!(
        query(&compositor, "status")["shell_corners"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    keys(&compositor, &[1]);
    assert_eq!(window(&compositor, a)["focused"], true);

    let dialog = add_unmapped(&mut client, Some(0), false);
    map(&mut client, dialog);
    let dialog_id = deferred_id(&compositor, dialog);
    assert_eq!(window(&compositor, dialog_id)["focused"], true);
    assert_focused(&compositor, &mut client, a);
    client.gtk_request_maximized(0, true);
    assert_eq!(window(&compositor, dialog_id)["focused"], true);
    assert_focused(&compositor, &mut client, a);
    action(&compositor, format!("focus-window {b}"));
    assert_eq!(window(&compositor, a)["focused_view"], false);
    action(&compositor, "pan-to 10 20");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
