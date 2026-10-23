//! Dragging a native group keeps its real controls above the group material.
use super::*;
use hearthspace_common::frame_layout::{
    FrameEdge, FrameGroup, FrameLayout, LayoutTarget, ObjectPlacement, TargetKind, WIDGETS,
    frame_group_app_id, frame_group_grip_app_id, frame_widget_app_id,
};

const CLOCK: [u8; 4] = [20, 20, 220, 255];
const WIFI: [u8; 4] = [220, 20, 20, 255];

fn group(state: &Value) -> &Value {
    state["outputs"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["id"] == "visible-controls")
        .unwrap()
}

fn assert_visible_controls(compositor: &HeadlessCompositor) {
    let state = layout(compositor);
    let mut decoder = png::Decoder::new(std::io::Cursor::new(compositor.take_screenshot()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
    let info = decoder.next_frame(&mut pixels).unwrap();
    let stride = info.color_type.samples();
    for (id, color) in [("clock", CLOCK), ("wifi", WIFI)] {
        let widget = state["outputs"][0]["widgets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|widget| widget["id"] == id)
            .unwrap();
        let rect = &widget["rect"];
        // Keep the sample away from the external grip's soft shadow on a narrow
        // vertical group. An incorrectly stacked backing still covers this point.
        let x = (rect["x"].as_f64().unwrap() + rect["width"].as_f64().unwrap() / 4.) as usize;
        let y = (rect["y"].as_f64().unwrap() + rect["height"].as_f64().unwrap() / 2.) as usize;
        let offset = (y * info.width as usize + x) * stride;
        let actual = &pixels[offset..offset + 3];
        let expected = [color[2], color[1], color[0]];
        // Allow the renderer's UNORM rounding while distinguishing each saturated
        // control from the opaque neutral backing that used to cover it.
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| actual.abs_diff(expected) <= 2),
            "the late-mapped group backing covered {id}: {actual:?}, expected {expected:?}; {state}"
        );
    }
}

#[test]
fn frame_layout_held_group_keeps_native_controls_above_late_mapped_backing() {
    let _guard = headless_test_guard();
    let saved = tempfile::tempdir().unwrap();
    let path = saved.path().join("layout.json");
    let mut model = FrameLayout {
        allow_hide_tools: true,
        ..Default::default()
    };
    for id in WIDGETS {
        model.set_placement(&LayoutTarget::widget(*id), ObjectPlacement::Hidden);
    }
    for id in ["clock", "wifi"] {
        model.placements.remove(&LayoutTarget::widget(id).key());
    }
    model.groups.insert(
        "visible-controls".into(),
        FrameGroup {
            id: "visible-controls".into(),
            members: ["clock", "wifi"]
                .into_iter()
                .map(LayoutTarget::widget)
                .collect(),
        },
    );
    model.set_placement(
        &LayoutTarget::new(TargetKind::Group, "visible-controls"),
        ObjectPlacement::frame(FrameEdge::Top, 0.75),
    );
    model.validate().unwrap();
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version":1,"x":0,"y":0,"scale":1.0,"placements":[],
            "frames":{"hearthspace-0":model},"allow_hide_tools":true,
        }))
        .unwrap(),
    )
    .unwrap();
    let mut compositor = HeadlessCompositor::spawn_with_layout("1280x800", Some(&path));
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion false");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let clock = client.add(
        frame_widget_app_id(Some(&output), "clock"),
        (70, 44),
        (0, 0),
        CLOCK,
    );
    client.add(
        frame_widget_app_id(Some(&output), "wifi"),
        (44, 44),
        (0, 0),
        WIFI,
    );
    // Real shell groups map their shared material separately from the controls.
    let backing = client.add(
        frame_group_app_id(Some(&output), "visible-controls"),
        (128, 54),
        (0, 0),
        [240, 240, 240, 255],
    );
    let grip = client.add(
        frame_group_grip_app_id(Some(&output), "visible-controls"),
        (28, 24),
        (0, 0),
        [20, 220, 20, 255],
    );
    action(&compositor, "layout-tool");
    assert_visible_controls(&compositor);
    let initial = layout(&compositor);
    let grip_rect = &group(&initial)["grip"];
    let x = grip_rect["x"].as_f64().unwrap() + 14.;
    let y = grip_rect["y"].as_f64().unwrap() + 12.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 800 450");
    assert_eq!(group(&layout(&compositor))["orientation"], "horizontal");
    assert_visible_controls(&compositor);

    action(&compositor, "pointer-motion-abs 20 400");
    client.resize(clock, (44, 70));
    client.resize(backing, (54, 128));
    client.resize(grip, (24, 28));
    assert_eq!(group(&layout(&compositor))["orientation"], "vertical");
    assert_visible_controls(&compositor);
    assert!(client.presses().is_empty());
    keys(&compositor, &[1]);
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
