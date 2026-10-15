//! New windows fit the current view without implicitly navigating the canvas.
use super::canvas_map_client::MapClient;
use super::navigation::{action, query, wait_windows};
use super::*;
use serde_json::Value;

fn viewport(compositor: &HeadlessCompositor) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let value = query(compositor, "status")["viewport"].clone();
        if value["animating"] == false {
            return value;
        }
        assert!(Instant::now() < deadline, "viewport did not settle");
        thread::sleep(Duration::from_millis(20));
    }
}

fn window(compositor: &HeadlessCompositor, app_id: &str) -> Value {
    wait_windows(compositor, |windows| {
        windows.iter().any(|window| window["app_id"] == app_id)
    })
    .into_iter()
    .find(|window| window["app_id"] == app_id)
    .unwrap()
}

pub(super) fn assert_centered(window: &Value, width: f64, height: f64) {
    let x = window["x"].as_f64().unwrap();
    let y = window["y"].as_f64().unwrap();
    let expected_x = ((width - window["width"].as_f64().unwrap()) / 2.0).max(0.0);
    let expected_y = 48.0 + ((height - 48.0 - window["height"].as_f64().unwrap()) / 2.0).max(0.0);
    assert!(
        (x - expected_x).abs() <= 1.0,
        "horizontal centering: {window}"
    );
    assert!(
        (y - expected_y).abs() <= 1.0,
        "vertical centering: {window}"
    );
}

#[test]
fn new_windows_center_at_current_pan_and_zoom_without_moving_viewport() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = MapClient::connect(&socket);
    for (number, scale) in [1.0, 0.8, 2.0].into_iter().enumerate() {
        action(&compositor, format!("zoom-to {scale}"));
        viewport(&compositor);
        action(&compositor, "pan-to 2400 -1100");
        let before = viewport(&compositor);
        let app_id = format!("dev.hearthspace.Placement{number}");
        let client_window = client.add(app_id.clone(), (420, 280), (14, 12), [60, 130, 190, 255]);
        let mapped = window(&compositor, &app_id);
        assert_centered(&mapped, 1280.0, 800.0);
        assert_eq!(viewport(&compositor), before, "mapping changed viewport");
        assert_eq!(
            client.focus(),
            Some(client_window),
            "new client receives keyboard focus"
        );
        action(&compositor, "key-down 30");
        action(&compositor, "key-up 30");
        assert_eq!(client.keys().last(), Some(&30));
        // Activation and later client geometry commits do not refocus, recenter,
        // or reinterpret an ordinary repaint as a new placement request.
        let id = mapped["id"].as_u64().unwrap();
        action(&compositor, format!("move-window {id} 3200 -800"));
        let moved = window(&compositor, &app_id);
        client.repaint_with_size(client_window, (520, 320));
        let resized = window(&compositor, &app_id);
        assert_eq!(resized["canvas_x"], moved["canvas_x"]);
        assert_eq!(resized["canvas_y"], moved["canvas_y"]);
        assert_eq!(resized["content_width"], 520);
        assert_eq!(viewport(&compositor), before, "repaint changed viewport");

        // Explicit focus is still deliberate canvas navigation.
        action(&compositor, format!("move-window {id} 9000 -9000"));
        action(&compositor, format!("focus-window {id}"));
        assert_ne!(viewport(&compositor), before);
        assert_eq!(window(&compositor, &app_id)["visible"], true);
    }
}
