//! Group presentation moves live content without changing its canonical home until release.
use super::*;

fn pin_group(
    compositor: &HeadlessCompositor,
    client: &mut super::super::canvas_map_client::MapClient,
) -> (u64, String) {
    client.add("group-member".into(), (500, 320), (0, 0), COLOR);
    let id = window(compositor, "group-member")["id"].as_u64().unwrap();
    action(compositor, format!("move-window {id} 2200 -1500"));
    action(compositor, "layout-tool");
    command(
        compositor,
        "place",
        json!({"kind":"group","id":"system"}),
        json!({"surface":"frame","edge":"right","position":0.5}),
    );
    command(
        compositor,
        "place",
        json!({"kind":"window","id":id.to_string()}),
        json!({"surface":"frame","edge":"right","position":0.5}),
    );
    let state = layout(compositor);
    let window_key = id.to_string();
    let group = state["outputs"][0]["layout"]["groups"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, group)| {
            group["members"]
                .as_array()
                .unwrap()
                .iter()
                .any(|member| member["kind"] == "window" && member["id"] == window_key)
        })
        .unwrap()
        .0
        .clone();
    (id, group)
}

#[test]
fn frame_layout_inspector_releases_group_members_to_both_canvas_layers_at_visible_slots() {
    let _guard = headless_test_guard();
    for surface in ["canvas", "background"] {
        let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
        drop(compositor.wait_for_socket());
        action(&compositor, "canvas-search-motion false");
        let mut client = connect(&compositor);
        let (id, group) = pin_group(&compositor, &mut client);
        let state = layout(&compositor);
        let slot = state["pinned_windows"][0]["rect"].clone();
        command(
            &compositor,
            "select",
            json!({"kind":"group","id":group}),
            Value::Null,
        );
        command(
            &compositor,
            "place",
            json!({"kind":"group","id":group}),
            json!({"surface":surface}),
        );
        let released = layout(&compositor);
        assert!(
            released["outputs"][0]["layout"]["groups"]
                .get(&group)
                .is_none(),
            "{released}"
        );
        assert!(released["selected"].is_null());
        assert_eq!(
            released["outputs"][0]["layout"]["placements"][format!("window:{id}")]["surface"],
            surface
        );
        assert_eq!(
            released["outputs"][0]["layout"]["placements"]["widget:wifi"]["surface"],
            surface
        );
        let moved = window(&compositor, "group-member");
        assert_eq!(
            moved["canvas_x"].as_f64().unwrap(),
            slot["x"].as_f64().unwrap().round()
        );
        assert_eq!(
            moved["canvas_y"].as_f64().unwrap(),
            slot["y"].as_f64().unwrap().round()
        );
        command(&compositor, "done", Value::Null, Value::Null);
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
}

#[test]
fn frame_layout_held_group_drag_moves_live_window_buffer_without_rewriting_home() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let (_, id) = pin_group(&compositor, &mut client);
    let before = window(&compositor, "group-member");
    let state = layout(&compositor);
    let grip = &state["outputs"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["id"] == id)
        .unwrap()["grip"];
    let x = grip["x"].as_f64().unwrap() + grip["width"].as_f64().unwrap() / 2.;
    let y = grip["y"].as_f64().unwrap() + grip["height"].as_f64().unwrap() / 2.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 450 250");
    let moved = window(&compositor, "group-member");
    assert_eq!(moved["canvas_x"], before["canvas_x"]);
    assert_eq!(moved["canvas_y"], before["canvas_y"]);
    assert_ne!(moved["x"], before["x"]);
    assert_eq!(moved["width"], 500);
    assert_eq!(moved["height"], 320);
    let mut decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut pixels).unwrap();
    let stride = info.color_type.samples();
    let x = moved["x"].as_f64().unwrap() + 20.;
    let y = moved["y"].as_f64().unwrap() + 20.;
    assert!(x >= 0. && y >= 0. && x < 1280. && y < 800., "{moved}");
    let offset = (y as usize * info.width as usize + x as usize) * stride;
    assert!(
        pixels[offset] > 150 && pixels[offset + 2] < 100,
        "held group does not display its live client buffer"
    );
    keys(&compositor, &[1]);
    action(&compositor, "pointer-button-up 272");
    let restored = window(&compositor, "group-member");
    assert_eq!(restored["canvas_x"], before["canvas_x"]);
    assert_eq!(restored["canvas_y"], before["canvas_y"]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_interrupted_member_press_reanchors_then_restores_its_canvas_home() {
    let _guard = headless_test_guard();
    for cancel in [false, true] {
        let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
        drop(compositor.wait_for_socket());
        action(&compositor, "canvas-search-motion false");
        let mut client = connect(&compositor);
        let (id, group_id) = pin_group(&compositor, &mut client);
        let original = window(&compositor, "group-member");
        let state = layout(&compositor);
        let output = state["output"].as_str().unwrap();
        let group = state["outputs"][0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["id"] == group_id)
            .unwrap();
        let bounds = &group["rect"];
        client.add(
            hearthspace_common::frame_layout::frame_group_app_id(Some(output), &group_id),
            (
                bounds["width"].as_f64().unwrap().round() as i32,
                bounds["height"].as_f64().unwrap().round() as i32,
            ),
            (0, 0),
            COLOR,
        );
        let slot = state["pinned_windows"][0]["rect"].clone();
        let x = slot["x"].as_f64().unwrap() + slot["width"].as_f64().unwrap() / 2.;
        let y = slot["y"].as_f64().unwrap() + slot["height"].as_f64().unwrap() / 2.;
        action(&compositor, "canvas-search-motion true");
        let _ = compositor.take_screenshot();
        command(
            &compositor,
            "place",
            json!({"kind":"group","id":group_id}),
            json!({"surface":"frame","edge":"top","position":0.5}),
        );
        thread::sleep(Duration::from_millis(350));
        assert_eq!(layout(&compositor)["animating"], true);
        action(&compositor, format!("pointer-motion-abs {x} {y}"));
        action(&compositor, "pointer-button-down 272");
        let pressed = layout(&compositor);
        assert_eq!(
            pressed["selected"]["target"],
            json!({"kind":"window","id":id.to_string()}),
            "{pressed}"
        );
        let grabbed = window(&compositor, "group-member");
        assert_eq!(
            grabbed["canvas_x"].as_f64().unwrap(),
            original["x"].as_f64().unwrap()
        );
        assert_eq!(
            grabbed["canvas_y"].as_f64().unwrap(),
            original["y"].as_f64().unwrap()
        );
        assert_ne!(grabbed["canvas_x"], original["canvas_x"]);
        assert!(client.presses().is_empty());
        if cancel {
            keys(&compositor, &[1]);
        }
        action(&compositor, "pointer-button-up 272");
        let restored = window(&compositor, "group-member");
        assert_eq!(restored["canvas_x"], original["canvas_x"]);
        assert_eq!(restored["canvas_y"], original["canvas_y"]);
        let member_id = id.to_string();
        assert_eq!(
            layout(&compositor)["outputs"][0]["layout"]["groups"][&group_id]["members"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|member| member["id"].as_str() == Some(member_id.as_str()))
                .count(),
            1
        );
        action(&compositor, "quit");
        wait_for_exit(&mut compositor.child);
    }
}
