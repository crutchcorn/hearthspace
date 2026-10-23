//! Release-time peeking through real clients, pointer grabs, and layout commands.
use super::canvas_map_client::MapClient;
use super::interactive::client::InteractiveClient;
use super::navigation::{action, keys, query, wait_windows};
use super::shell_panels::{click, connect};
use super::*;
use serde_json::{Value, json};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode;

#[path = "canvas_peek/decorations.rs"]
mod decorations;
#[path = "canvas_peek/gestures.rs"]
mod gestures;
#[path = "canvas_peek/maximize_transaction.rs"]
mod maximize_transaction;
#[path = "canvas_peek/presentation.rs"]
mod presentation;
#[path = "canvas_peek/transactions.rs"]
mod transactions;
#[path = "canvas_peek/windows.rs"]
mod windows;

const COLOR: [u8; 4] = [40, 180, 80, 255];

fn fixture() -> HeadlessCompositor {
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    compositor
}

fn interactive(compositor: &HeadlessCompositor) -> (InteractiveClient, [u64; 2]) {
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let client = InteractiveClient::connect_server_chrome(&socket);
    let windows = wait_windows(compositor, |windows| windows.len() == 2);
    let ids = [0, 1].map(|number| {
        windows
            .iter()
            .find(|w| w["title"] == format!("Interactive fixture {number}"))
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    });
    // Separate the initially mapped windows before constructing any coverage.
    action(compositor, format!("move-window {} 800 200", ids[1]));
    action(compositor, format!("move-window {} 200 200", ids[0]));
    (client, ids)
}

fn window(compositor: &HeadlessCompositor, id: u64) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"] == id)
        .unwrap()
        .clone()
}

fn geometry(window: &Value) -> [i64; 4] {
    ["canvas_x", "canvas_y", "content_width", "content_height"]
        .map(|field| window[field].as_i64().unwrap())
}

fn wait_position(compositor: &HeadlessCompositor, id: u64, x: i64, y: i64) -> Value {
    wait_windows(compositor, |windows| {
        windows
            .iter()
            .any(|w| w["id"] == id && w["canvas_x"] == x && w["canvas_y"] == y)
    })
    .into_iter()
    .find(|w| w["id"] == id)
    .unwrap()
}

fn normal(
    compositor: &HeadlessCompositor,
    client: &mut MapClient,
    app: &str,
    size: (i32, i32),
    position: (i32, i32),
) -> (usize, u64) {
    let number = client.add_decorated(app.into(), size, COLOR, Mode::ServerSide);
    let windows = wait_windows(compositor, |windows| {
        windows.iter().any(|w| w["app_id"] == app)
    });
    let id = windows.iter().find(|w| w["app_id"] == app).unwrap()["id"]
        .as_u64()
        .unwrap();
    action(
        compositor,
        format!("move-window {id} {} {}", position.0, position.1),
    );
    (number, id)
}

/// Leave the button held so callers can inspect the fully covered drag sample.
fn drag_header_to(compositor: &HeadlessCompositor, id: u64, target: (i64, i64)) {
    let before = window(compositor, id);
    let scale = query(compositor, "status")["viewport"]["scale"]
        .as_f64()
        .unwrap();
    assert_eq!(
        before["titlebar_height"].as_f64().unwrap(),
        (44.0 * scale).round()
    );
    let x = before["x"].as_f64().unwrap() + 40.0 * scale;
    let y = before["y"].as_f64().unwrap() + 22.0 * scale;
    action(compositor, format!("pointer-motion-abs {x} {y}"));
    action(compositor, "pointer-button-down 272");
    let x = x + (target.0 - before["canvas_x"].as_i64().unwrap()) as f64 * scale;
    let y = y + (target.1 - before["canvas_y"].as_i64().unwrap()) as f64 * scale;
    action(compositor, format!("pointer-motion-abs {x} {y}"));
}

fn layout_command(compositor: &HeadlessCompositor, name: &str, target: Value, placement: Value) {
    let state = query(compositor, "layout-status");
    action(
        compositor,
        format!(
            "layout-command {}",
            json!({
                "session":state["session"], "output":state["output"],
                "action":name, "target":target, "placement":placement,
            })
        ),
    );
}

fn place(compositor: &HeadlessCompositor, target: Value, surface: &str, x: f64, y: f64) {
    action(compositor, "layout-tool");
    layout_command(
        compositor,
        "place",
        target,
        json!({"surface":surface,"x":x,"y":y}),
    );
    layout_command(compositor, "done", Value::Null, Value::Null);
}

fn item_position(compositor: &HeadlessCompositor, id: &str) -> [i64; 2] {
    let state = query(compositor, "desktop-status");
    let item = state["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap();
    [item["x"].as_i64().unwrap(), item["y"].as_i64().unwrap()]
}

fn wait_item(compositor: &HeadlessCompositor, id: &str, expected: [i64; 2]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let actual = item_position(compositor, id);
        if actual == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "item {id}: expected {expected:?}, got {actual:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn shutdown(mut compositor: HeadlessCompositor) {
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
