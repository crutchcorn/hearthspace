//! Output-qualified map controls preserve geometry and navigation admission.
use hearthspace_ipc::command::{ShellCommand, ZoomOperation};

use super::*;
use crate::navigation::{keys, wait_windows};

fn command(output: &str, operation: ZoomOperation) -> String {
    ShellCommand::ZoomOnOutput {
        output_name: output.into(),
        operation,
    }
    .wire_name()
}

fn canvas_center(viewport: &Value, output: &Value) -> (f64, f64) {
    let scale = viewport["scale"].as_f64().unwrap();
    (
        viewport["x"].as_f64().unwrap()
            + (output["logical_x"].as_f64().unwrap()
                + output["logical_width"].as_f64().unwrap() / 2.0)
                / scale,
        viewport["y"].as_f64().unwrap()
            + (output["logical_y"].as_f64().unwrap()
                + output["logical_height"].as_f64().unwrap() / 2.0)
                / scale,
    )
}

fn zoom_keeps_output_center(
    compositor: &HeadlessCompositor,
    output: &Value,
    operation: ZoomOperation,
    expected_scale: f64,
) {
    let before = query(compositor, "status")["viewport"].clone();
    action(
        compositor,
        command(output["name"].as_str().unwrap(), operation),
    );
    crate::canvas_zoom::wait_scale(compositor, expected_scale);
    let after = query(compositor, "status")["viewport"].clone();
    let before_center = canvas_center(&before, output);
    let after_center = canvas_center(&after, output);
    // Canvas offsets are integral; each axis rounds only once at the endpoint.
    assert!((before_center.0 - after_center.0).abs() <= 0.5 + 1e-9);
    assert!((before_center.1 - after_center.1).abs() <= 0.5 + 1e-9);
}

#[test]
fn output_map_zoom_preserves_center_client_home_focus_and_popup_bounds() {
    let _guard = headless_test_guard();
    // Odd logical dimensions require fractional centers even on a 2x output.
    let mut compositor = HeadlessCompositor::spawn_with_options("2562x1602", None, None, 2);
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0].clone();
    assert_eq!(output["logical_width"], 1281);
    assert_eq!(output["logical_height"], 801);
    let mut client = connect(&compositor);
    client.add(
        "dev.hearthspace.MapZoomTest".into(),
        (240, 160),
        (0, 0),
        GREEN,
    );
    let map = client.add(
        canvas_map_app_id_for_output(output["name"].as_str().unwrap()),
        (352, 312),
        (0, 0),
        GREEN,
    );
    action(&compositor, "pan-to -321 467");
    let home = query(&compositor, "windows")["windows"][0].clone();
    let bounds = map_bounds(&compositor);
    zoom_keeps_output_center(&compositor, &output, ZoomOperation::Out, 0.8);
    zoom_keeps_output_center(&compositor, &output, ZoomOperation::To(1.0), 1.0);
    zoom_keeps_output_center(&compositor, &output, ZoomOperation::In, 1.25);
    zoom_keeps_output_center(&compositor, &output, ZoomOperation::To(1.0), 1.0);
    assert_eq!(client.focus(), Some(map));
    assert!(client.closed().is_empty());
    assert_eq!(map_bounds(&compositor), bounds);
    let after = query(&compositor, "windows")["windows"][0].clone();
    for field in [
        "canvas_x",
        "canvas_y",
        "content_width",
        "content_height",
        "maximized",
        "fullscreen",
    ] {
        assert_eq!(after[field], home[field], "map zoom changed {field}");
    }

    zoom_keeps_output_center(&compositor, &output, ZoomOperation::To(0.5), 0.5);
    let at_limit = query(&compositor, "status")["viewport"].clone();
    zoom_keeps_output_center(&compositor, &output, ZoomOperation::Out, 0.5);
    assert_eq!(query(&compositor, "status")["viewport"], at_limit);
    zoom_keeps_output_center(&compositor, &output, ZoomOperation::To(2.0), 2.0);
    let at_limit = query(&compositor, "status")["viewport"].clone();
    zoom_keeps_output_center(&compositor, &output, ZoomOperation::In, 2.0);
    assert_eq!(query(&compositor, "status")["viewport"], at_limit);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

fn assert_rejected(compositor: &HeadlessCompositor, output: &str, reason: &str) {
    let before = query(compositor, "status")["viewport"].clone();
    for operation in [
        ZoomOperation::In,
        ZoomOperation::Out,
        ZoomOperation::To(1.0),
    ] {
        let reply = compositor.send_text_command(&command(output, operation));
        assert!(reply.starts_with(reason), "unexpected rejection: {reply}");
        assert_eq!(query(compositor, "status")["viewport"], before);
    }
}

#[test]
fn output_map_zoom_rejects_unavailable_outputs_invalid_scales_overlays_focused_view_and_lock() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    let output = query(&compositor, "status")["outputs"][0]["name"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let normal = client.add(
        "dev.hearthspace.MapZoomGuardTest".into(),
        (240, 160),
        (0, 0),
        GREEN,
    );
    let map = client.add(
        canvas_map_app_id_for_output(&output),
        (352, 312),
        (0, 0),
        GREEN,
    );
    action(&compositor, "zoom-to 0.75");
    crate::canvas_zoom::wait_scale(&compositor, 0.75);
    action(&compositor, "pan-to -321 467");
    assert_rejected(
        &compositor,
        "Disconnected output 二",
        "err output is unavailable",
    );
    let before = query(&compositor, "status")["viewport"].clone();
    for scale in [0.49, 2.01] {
        assert!(
            compositor
                .send_text_command(&command(&output, ZoomOperation::To(scale)))
                .starts_with("err zoom must be between")
        );
        assert_eq!(query(&compositor, "status")["viewport"], before);
    }
    assert_eq!(client.focus(), Some(map));
    assert!(client.closed().is_empty());
    client.destroy(map);
    assert_eq!(client.focus(), Some(normal));

    keys(&compositor, &[125, 42, 33]); // Super+Shift+F enters focused view.
    client.sync();
    wait_windows(&compositor, |windows| windows[0]["focused_view"] == true);
    wait_animation(&compositor);
    assert_rejected(
        &compositor,
        &output,
        "err canvas navigation is unavailable in focused view",
    );
    keys(&compositor, &[125, 42, 33]);
    client.sync();
    wait_windows(&compositor, |windows| windows[0]["focused_view"] == false);
    wait_animation(&compositor);

    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    wait_animation(&compositor);
    assert_rejected(&compositor, &output, "err dismiss the window overlay");
    keys(&compositor, &[1]);
    action(&compositor, "key-up 56");
    wait_animation(&compositor);

    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = crate::session_lock::lock_client::LockClient::connect(&socket);
    locker.wait_locked();
    assert_rejected(&compositor, &output, "err session is locked");
    locker.unlock();
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
