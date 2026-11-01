//! Super+wheel keeps the same real client content below a stationary mouse.
use super::*;
use crate::canvas_map_client::MapClient;

#[test]
fn super_wheel_keeps_client_content_under_the_mouse_on_a_hidpi_output() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_options("2560x1600", None, None, 2);
    drop(compositor.wait_for_socket());
    let mut client = MapClient::connect(
        &compositor
            .runtime_dir
            .path()
            .join(format!("hearthspace-test-{}", std::process::id())),
    );
    let window = client.add(
        "dev.hearthspace.PointerZoomTest".into(),
        (320, 240),
        (0, 0),
        [0, 204, 102, 255],
    );
    let mapped = wait_windows(&compositor, |windows| windows.len() == 1)[0].clone();
    let id = mapped["id"].as_u64().unwrap();
    action(&compositor, "pan-to -321 467");
    action(&compositor, format!("move-window {id} -100 600"));
    let before = wait_windows(&compositor, |windows| {
        windows[0]["canvas_x"] == -100 && windows[0]["canvas_y"] == 600
    })[0]
        .clone();
    let x = before["x"].as_f64().unwrap() + 61.25;
    let y = before["y"].as_f64().unwrap() + 83.5;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    let first_press = *client.presses().last().unwrap();
    assert_eq!(first_press.0, window);
    let viewport = query(&compositor, "status")["viewport"].clone();
    let canvas = (
        viewport["x"].as_f64().unwrap() + x / viewport["scale"].as_f64().unwrap(),
        viewport["y"].as_f64().unwrap() + y / viewport["scale"].as_f64().unwrap(),
    );

    action(&compositor, "key-down 125");
    action(&compositor, "axis 0 -60");
    action(&compositor, "key-up 125");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = query(&compositor, "status")["viewport"].clone();
        let scale = current["scale"].as_f64().unwrap();
        let anchored = (
            current["x"].as_f64().unwrap() + x / scale,
            current["y"].as_f64().unwrap() + y / scale,
        );
        assert!(
            (canvas.0 - anchored.0).abs() <= 0.5 + 1e-6,
            "x drift: {current}"
        );
        assert!(
            (canvas.1 - anchored.1).abs() <= 0.5 + 1e-6,
            "y drift: {current}"
        );
        if current["animating"] == false {
            assert!(scale > viewport["scale"].as_f64().unwrap());
            break;
        }
        assert!(Instant::now() < deadline, "zoom never settled: {current}");
        thread::sleep(Duration::from_millis(10));
    }
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-button-up 272");
    let presses = client.presses();
    assert_eq!(presses.len(), 2);
    let last_press = presses[1];
    assert_eq!(last_press.0, window);
    assert!((first_press.1.0 - last_press.1.0).abs() <= 1.5);
    assert!((first_press.1.1 - last_press.1.1).abs() <= 1.5);
    let after = query(&compositor, "windows")["windows"][0].clone();
    for field in [
        "id",
        "canvas_x",
        "canvas_y",
        "content_width",
        "content_height",
    ] {
        assert_eq!(before[field], after[field], "zoom changed {field}");
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
