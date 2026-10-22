//! Attached window headers tuck their app without creating a maximize transaction.
use super::*;
use hearthspace_common::frame_layout::pinned_window_reveal_app_id;
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::Mode;

const APP: &str = "dev.hearthspace.FrameTitlebarTest";

fn pinned(compositor: &HeadlessCompositor, id: u64) -> Value {
    layout(compositor)["pinned_windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap()
        .clone()
}

fn assert_home(compositor: &HeadlessCompositor, original: &Value) {
    let current = window(compositor, APP);
    for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
        assert_eq!(
            current[field], original[field],
            "attachment changed {field}"
        );
    }
    for flag in ["maximized", "focused_view", "fullscreen"] {
        assert_eq!(current[flag], false, "attached header changed {flag}");
    }
}

#[test]
fn frame_attached_ssd_headers_tuck_to_native_reveal_focus_without_leaking_input() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut app = connect(&compositor);
    let app_surface = app.add_decorated(APP.into(), (500, 320), COLOR, Mode::ServerSide);
    let id = window(&compositor, APP)["id"].as_u64().unwrap();
    action(&compositor, format!("move-window {id} 2400 -1600"));
    let original = window(&compositor, APP);
    assert_eq!(original["titlebar_height"], 44);
    let viewport = query(&compositor, "status")["viewport"].clone();
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "place",
        json!({"kind":"window", "id":id.to_string()}),
        // Leave room for the default system group; overlapping it would
        // intentionally group the app and put its reveal strip below the header.
        json!({"surface":"frame", "edge":"top", "position":0.2}),
    );
    command(&compositor, "done", Value::Null, Value::Null);
    assert_eq!(pinned(&compositor, id)["rect"]["y"], 0.0);
    assert_eq!(pinned(&compositor, id)["rect"]["height"], 42.0);
    let mut reveal = connect(&compositor);
    let reveal_surface = reveal.add(pinned_window_reveal_app_id(id), (500, 42), (0, 0), COLOR);

    for double_click in [false, true] {
        action(&compositor, format!("frame-reveal {id}"));
        assert_eq!(pinned(&compositor, id)["expanded"], true);
        assert_eq!(app.focus(), Some(app_surface));
        // Positive control: ordinary keys do reach this same real app when open.
        let before_keys = app.keys().len();
        keys(&compositor, &[30]);
        let received = app.keys().len();
        assert_eq!(received, before_keys + 2);
        let strip_keys = reveal.keys().len();
        let expanded = window(&compositor, APP);
        let x = expanded["x"].as_f64().unwrap()
            + if double_click {
                80.
            } else {
                expanded["width"].as_f64().unwrap() - 58.
            };
        let y = expanded["y"].as_f64().unwrap() + 22.;
        super::super::shell_panels::click(&compositor, x, y);
        if double_click {
            super::super::shell_panels::click(&compositor, x, y);
        }
        let tucked = pinned(&compositor, id);
        assert_eq!(
            tucked["expanded"], false,
            "header action did not tuck: {tucked}"
        );
        // A top-edge header is also the hover region. Tucking changes the
        // committed state while the reference's wider hover preview stays open.
        assert_eq!(
            tucked["hovered"], true,
            "double_click={double_click}, pointer=({x},{y}), expanded={expanded}, tucked={tucked}"
        );
        assert_eq!(app.focus(), None);
        assert_eq!(reveal.focus(), Some(reveal_surface));
        keys(&compositor, &[48]);
        assert_eq!(
            app.keys().len(),
            received,
            "tucked app received keyboard input"
        );
        assert_eq!(&reveal.keys()[strip_keys..], &[48, 48]);
        assert_home(&compositor, &original);
        assert_eq!(query(&compositor, "status")["viewport"], viewport);
        action(&compositor, "pointer-motion-abs 600 600");
        assert_eq!(pinned(&compositor, id)["hovered"], false);
        assert_eq!(pinned(&compositor, id)["expanded"], false);
    }

    // If the strip is not mapped, tucking still must release client focus.
    reveal.destroy(reveal_surface);
    action(&compositor, format!("frame-reveal {id}"));
    let expanded = window(&compositor, APP);
    let x = expanded["x"].as_f64().unwrap() + expanded["width"].as_f64().unwrap() - 58.;
    let y = expanded["y"].as_f64().unwrap() + 22.;
    let received = app.keys().len();
    super::super::shell_panels::click(&compositor, x, y);
    assert_eq!(pinned(&compositor, id)["expanded"], false);
    assert_eq!(app.focus(), None);
    keys(&compositor, &[46]);
    assert_eq!(app.keys().len(), received);
    assert_home(&compositor, &original);
    assert!(app.presses().is_empty(), "header press reached app content");
    assert!(
        app.releases().is_empty(),
        "header release reached app content"
    );
    assert!(app.closed().is_empty());
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
