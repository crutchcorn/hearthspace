use super::*;

fn widget(state: &Value, id: &str) -> Value {
    state["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["id"] == id)
        .unwrap()
        .clone()
}

#[test]
fn frame_layout_expanded_controls_keep_native_width_and_canvas_preserves_screen_position() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    client.add(
        hearthspace_common::frame_layout::frame_widget_app_id(Some(&output), "clock"),
        (94, 44),
        (0, 0),
        COLOR,
    );
    let wifi = client.add(
        hearthspace_common::frame_layout::frame_widget_app_id(Some(&output), "wifi"),
        (100, 44),
        (0, 0),
        COLOR,
    );
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "place",
        json!({"kind":"group","id":"system"}),
        json!({"surface":"frame","edge":"left","position":0.5}),
    );
    let state = layout(&compositor);
    assert_eq!(widget(&state, "clock")["rect"]["width"], 44.0, "{state}");
    assert_eq!(widget(&state, "clock")["rect"]["height"], 94.0, "{state}");
    assert_eq!(widget(&state, "wifi")["rect"]["width"], 100.0, "{state}");
    command(
        &compositor,
        "place",
        json!({"kind":"widget","id":"wifi"}),
        json!({"surface":"frame","edge":"right","position":0.7}),
    );
    client.repaint_with_size(wifi, (110, 54));
    let before = widget(&layout(&compositor), "wifi")["rect"].clone();
    assert_eq!(before["width"], 110.0);
    assert_eq!(before["height"], 54.0);
    command(
        &compositor,
        "place",
        json!({"kind":"widget","id":"wifi"}),
        json!({"surface":"canvas"}),
    );
    let canvas = widget(&layout(&compositor), "wifi")["rect"].clone();
    assert_eq!(
        canvas, before,
        "Canvas placement should retain the visible rectangle"
    );
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_grouped_pinned_buffer_reaches_slot_inward_edge() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    client.add("grouped-pin-buffer".into(), (500, 320), (0, 0), COLOR);
    let id = window(&compositor, "grouped-pin-buffer")["id"]
        .as_u64()
        .unwrap();
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "place",
        json!({"kind":"group","id":"system"}),
        json!({"surface":"frame","edge":"right","position":0.5}),
    );
    // Overlap the group so the same settlement path used by a pointer drop joins it.
    command(
        &compositor,
        "place",
        json!({"kind":"window","id":id.to_string()}),
        json!({"surface":"frame","edge":"right","position":0.5}),
    );
    let state = layout(&compositor);
    let slot = &state["pinned_windows"][0]["rect"];
    let projected = window(&compositor, "grouped-pin-buffer");
    let x = slot["x"].as_f64().unwrap();
    assert!(
        x < 1238.,
        "window must be grouped, not a standalone 42px strip: {state}"
    );
    assert_eq!(projected["x"].as_f64().unwrap(), x, "{projected}");
    let pixels = compositor.take_screenshot();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(pixels))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut pixels).unwrap();
    let stride = info.color_type.samples();
    let offset = (((slot["y"].as_f64().unwrap() + 20.) as usize) * info.width as usize
        + (x + 3.) as usize)
        * stride;
    let sample = &pixels[offset..offset + stride];
    assert!(
        sample[0] > 150 && sample[2] < 100,
        "slot does not expose the live client buffer: {sample:?}"
    );
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_desktop_item_resizes_its_existing_surface_when_pinned_grouped_and_unpinned() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let mut client = connect(&compositor);
    let id = "file:50726f6a65637473";
    let target = json!({"kind":"item","id":id});
    let surface = client.add(
        hearthspace_common::desktop_item_app_id(id),
        (100, 108),
        (0, 0),
        COLOR,
    );
    let home = query(&compositor, "desktop-status")["items"][0].clone();
    assert_eq!(client.size(surface), (100, 108));
    action(&compositor, "layout-tool");
    command(
        &compositor,
        "place",
        target.clone(),
        json!({"surface":"frame","edge":"right","position":0.5}),
    );
    // Match the native shell's changing fixed-size hints. Do not force a new
    // buffer: the compositor must configure this existing surface to its size.
    client.resize(surface, (54, 54));
    assert_eq!(client.size(surface), (54, 54));

    command(
        &compositor,
        "place",
        json!({"kind":"group","id":"system"}),
        json!({"surface":"frame","edge":"right","position":0.5}),
    );
    let state = layout(&compositor);
    assert!(
        state["outputs"][0]["layout"]["groups"]
            .as_object()
            .unwrap()
            .values()
            .any(|group| group["members"].as_array().unwrap().contains(&target)),
        "the frame item should join the overlapping group: {state}"
    );
    client.resize(surface, (44, 44));
    assert_eq!(client.size(surface), (44, 44));

    command(&compositor, "place", target, json!({"surface":"canvas"}));
    client.resize(surface, (100, 108));
    assert_eq!(client.size(surface), (100, 108));
    let restored = query(&compositor, "desktop-status")["items"][0].clone();
    assert_eq!(restored["id"], home["id"]);
    assert_eq!(restored["x"], home["x"]);
    assert_eq!(restored["y"], home["y"]);
    assert!(client.closed().is_empty());
    command(&compositor, "done", Value::Null, Value::Null);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
