//! Canvas maximization follows a multi-window drag without discarding restore geometry.
use super::super::{interactive::client::InteractiveClient, navigation::wait_windows};
use super::*;

fn pixel(compositor: &HeadlessCompositor, name: &str, x: i64, y: i64) -> [u8; 3] {
    let png = compositor.take_screenshot();
    std::fs::write(compositor.runtime_dir.path().join(name), &png).unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let offset = (y as usize * info.width as usize + x as usize) * info.color_type.samples();
    pixels[offset..offset + 3].try_into().unwrap()
}

fn assert_translation(before: &Value, after: &Value, screen: (i64, i64), canvas: (i64, i64)) {
    for (field, delta) in [
        ("x", screen.0),
        ("y", screen.1),
        ("canvas_x", canvas.0),
        ("canvas_y", canvas.1),
    ] {
        assert_eq!(
            after[field].as_i64().unwrap(),
            before[field].as_i64().unwrap() + delta,
            "translated {field}"
        );
    }
    for field in [
        "width",
        "height",
        "content_width",
        "content_height",
        "maximized",
    ] {
        assert_eq!(after[field], before[field], "preserved {field}");
    }
}

#[test]
fn group_drag_carries_canvas_maximized_bounds_and_restore_home_at_both_scales() {
    let _guard = headless_test_guard();
    for (scale, cancel) in [(1.0, false), (0.8, true)] {
        let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
        drop(compositor.wait_for_socket());
        let socket = compositor
            .runtime_dir
            .path()
            .join(format!("hearthspace-test-{}", std::process::id()));
        let mut client = InteractiveClient::connect_server_chrome(&socket);
        let windows = wait_windows(&compositor, |windows| windows.len() == 2);
        let id = |number| {
            windows
                .iter()
                .find(|window| window["title"] == format!("Interactive fixture {number}"))
                .unwrap()["id"]
                .as_u64()
                .unwrap()
        };
        let (maximized, ordinary) = (id(0), id(1));
        for (id, x) in [(maximized, 120), (ordinary, 650)] {
            action(&compositor, format!("move-window {id} {x} 200"));
        }
        action(&compositor, format!("zoom-to {scale}"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while query(&compositor, "status")["viewport"]["animating"] == true {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        action(&compositor, "pan-to 0 0");
        let original = window(&compositor, maximized);
        action(&compositor, format!("maximize-window {maximized} true"));
        client.sync();
        wait_windows(&compositor, |windows| {
            windows
                .iter()
                .any(|window| window["id"] == maximized && window["width"] == 1232)
        });

        select_by_keyboard(&compositor, maximized);
        let alone = window(&compositor, maximized);
        let x = alone["x"].as_f64().unwrap() + 180.0;
        let y = alone["y"].as_f64().unwrap() + 22.0 * scale;
        action(&compositor, format!("pointer-motion-abs {x} {y}"));
        action(&compositor, "pointer-button-down 272");
        action(
            &compositor,
            format!("pointer-motion-abs {} {}", x + 40.0, y + 20.0),
        );
        action(&compositor, "pointer-button-up 272");
        assert_translation(&alone, &window(&compositor, maximized), (0, 0), (0, 0));

        action(&compositor, "group-selection-toggle");
        for code in [15, 57, 28] {
            keys(&compositor, &[code]);
        }
        assert_eq!(group(&compositor)["selected"], json!([maximized, ordinary]));
        action(&compositor, format!("focus-window {ordinary}"));
        action(&compositor, "pan-to 0 0");
        client.sync();
        let before = [
            window(&compositor, maximized),
            window(&compositor, ordinary),
        ];
        let sample = (
            before[0]["x"].as_i64().unwrap() + 12,
            before[0]["y"].as_i64().unwrap() + 150,
        );
        assert_eq!(
            pixel(
                &compositor,
                "maximized-before-group-drag.png",
                sample.0,
                sample.1
            ),
            [0, 204, 102]
        );
        let x = before[1]["x"].as_f64().unwrap() + 40.0 * scale;
        let y = before[1]["y"].as_f64().unwrap() + 22.0 * scale;
        action(&compositor, format!("pointer-motion-abs {x} {y}"));
        action(&compositor, "pointer-button-down 272");
        for (dx, dy) in [(48, 24), (96, 48)] {
            action(
                &compositor,
                format!(
                    "pointer-motion-abs {} {}",
                    x + f64::from(dx),
                    y + f64::from(dy)
                ),
            );
            let canvas_delta = (
                (f64::from(dx) / scale).round() as i64,
                (f64::from(dy) / scale).round() as i64,
            );
            for (before, id) in before.iter().zip([maximized, ordinary]) {
                assert_translation(
                    before,
                    &window(&compositor, id),
                    (i64::from(dx), i64::from(dy)),
                    canvas_delta,
                );
            }
            let bounds = window(&compositor, maximized)["maximized_canvas_bounds"].clone();
            for (axis, delta) in [("x", canvas_delta.0), ("y", canvas_delta.1)] {
                assert_eq!(
                    bounds[axis].as_i64().unwrap(),
                    before[0]["maximized_canvas_bounds"][axis].as_i64().unwrap() + delta
                );
            }
        }
        assert_ne!(
            pixel(
                &compositor,
                "maximized-held-old-location.png",
                sample.0,
                sample.1
            ),
            [0, 204, 102]
        );
        assert_eq!(
            pixel(
                &compositor,
                "maximized-held-new-location.png",
                sample.0 + 96,
                sample.1 + 48
            ),
            [0, 204, 102]
        );

        let held = window(&compositor, maximized);
        if cancel {
            keys(&compositor, &[1]);
        }
        action(
            &compositor,
            format!("pointer-motion-abs {} {}", x + 140.0, y + 70.0),
        );
        // Escape cancels the gesture at its last applied location. Normal release
        // is performed at that same location to check identical restore semantics.
        if !cancel {
            action(
                &compositor,
                format!("pointer-motion-abs {} {}", x + 96.0, y + 48.0),
            );
        }
        action(&compositor, "pointer-button-up 272");
        assert_translation(&held, &window(&compositor, maximized), (0, 0), (0, 0));
        keys(&compositor, &[1]);
        action(&compositor, format!("restore-window {maximized}"));
        client.sync();
        let restored = window(&compositor, maximized);
        assert_eq!(restored["maximized"], false);
        assert_eq!(restored["maximized_canvas_bounds"], Value::Null);
        for field in ["canvas_x", "canvas_y"] {
            assert_eq!(restored[field], held[field]);
        }
        for field in ["content_width", "content_height"] {
            assert_eq!(restored[field], original[field]);
        }
        assert!(
            !client.received_pointer_press(),
            "group/chrome press leaked to client"
        );
        assert!(
            client.released_buttons().is_empty(),
            "group/chrome release leaked to client"
        );
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
}
