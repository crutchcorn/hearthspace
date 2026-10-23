//! A mixed group keeps one grip and final shape while native members reflow asynchronously.
use super::*;
use hearthspace_common::frame_layout::{
    FrameEdge, FrameGroup, FrameLayout, LayoutTarget, ObjectPlacement, TargetKind, WIDGETS,
    frame_group_app_id, frame_group_grip_app_id, frame_widget_app_id,
};

fn group(state: &Value) -> &Value {
    state["outputs"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|group| group["id"] == "mixed")
        .unwrap()
}

fn widget(state: &Value, id: &str) -> Value {
    state["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["id"] == id)
        .unwrap()
        .clone()
}

fn anchored_column(state: &Value, clock_height: f64) {
    let width = 54.;
    let height = 330. + clock_height;
    let rect = &group(state)["rect"];
    assert_eq!(rect["width"], width, "{state}");
    assert_eq!(rect["height"], height, "{state}");
    // The horizontal grip is18px below its54px body: across=4/3, not clamped1.
    let pivot_x = -width / 3.;
    assert!(
        (rect["x"].as_f64().unwrap() + pivot_x - 20.).abs() < 0.001,
        "{rect}"
    );
    assert!(
        (rect["y"].as_f64().unwrap() + height / 2. - 400.).abs() < 0.001,
        "{rect}"
    );
    assert_eq!(widget(state, "tools")["rect"]["width"], 44.);
    assert_eq!(widget(state, "tools")["rect"]["height"], 132.);
    assert_eq!(widget(state, "apps")["rect"]["width"], 44.);
    assert_eq!(widget(state, "apps")["rect"]["height"], 88.);
    assert_eq!(widget(state, "tray")["rect"]["width"], 44.);
    assert_eq!(widget(state, "tray")["rect"]["height"], 88.);
    assert_eq!(widget(state, "clock")["rect"]["width"], 44.);
    assert_eq!(widget(state, "clock")["rect"]["height"], clock_height);
    assert_eq!(group(state)["grip"]["width"], 24.);
    assert_eq!(group(state)["grip"]["height"], 28.);
}

#[test]
fn frame_layout_mixed_group_turn_waits_for_native_reflow_and_preserves_external_grip() {
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
    model.groups.insert(
        "mixed".into(),
        FrameGroup {
            id: "mixed".into(),
            members: ["clock", "tools", "apps", "tray"]
                .into_iter()
                .map(LayoutTarget::widget)
                .collect(),
        },
    );
    for member in &model.groups["mixed"].members {
        model.placements.remove(&member.key());
    }
    model.set_placement(
        &LayoutTarget::new(TargetKind::Group, "mixed"),
        ObjectPlacement::frame(FrameEdge::Top, 0.5),
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
    action(&compositor, "canvas-search-motion true");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let clock = client.add(
        frame_widget_app_id(Some(&output), "clock"),
        (80, 44),
        (0, 0),
        COLOR,
    );
    let tools = client.add(
        frame_widget_app_id(Some(&output), "tools"),
        (132, 44),
        (0, 0),
        COLOR,
    );
    let apps = client.add(
        frame_widget_app_id(Some(&output), "apps"),
        (88, 44),
        (0, 0),
        COLOR,
    );
    let tray = client.add(
        frame_widget_app_id(Some(&output), "tray"),
        (88, 44),
        (0, 0),
        COLOR,
    );
    let backing = client.add(
        frame_group_app_id(Some(&output), "mixed"),
        (410, 54),
        (0, 0),
        COLOR,
    );
    action(&compositor, "layout-tool");
    let grip_surface = client.add(
        frame_group_grip_app_id(Some(&output), "mixed"),
        (28, 24),
        (0, 0),
        COLOR,
    );
    let initial = layout(&compositor);
    let grip = &group(&initial)["grip"];
    let x = grip["x"].as_f64().unwrap() + grip["width"].as_f64().unwrap() / 2.;
    let y = grip["y"].as_f64().unwrap() + grip["height"].as_f64().unwrap() / 2.;
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    action(&compositor, "pointer-motion-abs 20 400");
    anchored_column(&layout(&compositor), 80.);
    assert_eq!(
        client.size(tools),
        (132, 44),
        "old native pixels have not reflowed yet"
    );
    thread::sleep(Duration::from_millis(350));
    assert_eq!(
        layout(&compositor)["animating"],
        true,
        "pending reflow consumed the turn's duration"
    );

    client.resize(clock, (44, 110));
    anchored_column(&layout(&compositor), 110.);
    client.resize(tools, (44, 132));
    client.resize(apps, (44, 88));
    client.resize(tray, (44, 88));
    let before_backing = layout(&compositor);
    anchored_column(&before_backing, 110.);
    assert_eq!(before_backing["animating"], true);
    client.resize(backing, (54, 440));
    assert_eq!(layout(&compositor)["animating"], true);
    client.resize(grip_surface, (24, 28));
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let state = layout(&compositor);
        anchored_column(&state, 110.);
        if state["animating"] == false {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "coherent native buffers did not finish the turn: {state}"
        );
        thread::sleep(Duration::from_millis(10));
    }
    keys(&compositor, &[1]);
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_pending_group_turn_stops_when_a_member_is_hidden_or_destroyed() {
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
    model.groups.insert(
        "mixed".into(),
        FrameGroup {
            id: "mixed".into(),
            members: ["tools", "tray"]
                .into_iter()
                .map(LayoutTarget::widget)
                .collect(),
        },
    );
    for member in &model.groups["mixed"].members {
        model.placements.remove(&member.key());
    }
    model.set_placement(
        &LayoutTarget::new(TargetKind::Group, "mixed"),
        ObjectPlacement::frame(FrameEdge::Top, 0.5),
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
    action(&compositor, "canvas-search-motion true");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let tools = client.add(
        frame_widget_app_id(Some(&output), "tools"),
        (132, 44),
        (0, 0),
        COLOR,
    );
    let tray = client.add(
        frame_widget_app_id(Some(&output), "tray"),
        (88, 44),
        (0, 0),
        COLOR,
    );
    client.add(
        frame_group_app_id(Some(&output), "mixed"),
        (234, 54),
        (0, 0),
        COLOR,
    );
    for destroyed in [false, true] {
        action(&compositor, "layout-tool");
        let initial = layout(&compositor);
        let grip = &group(&initial)["grip"];
        let x = grip["x"].as_f64().unwrap() + grip["width"].as_f64().unwrap() / 2.;
        let y = grip["y"].as_f64().unwrap() + grip["height"].as_f64().unwrap() / 2.;
        action(&compositor, format!("pointer-motion-abs {x} {y}"));
        action(&compositor, "pointer-button-down 272");
        action(&compositor, "pointer-motion-abs 20 400");
        thread::sleep(Duration::from_millis(350));
        assert_eq!(
            layout(&compositor)["animating"],
            true,
            "turn must still await reflow"
        );
        if destroyed {
            client.destroy(tray);
        } else {
            action(
                &compositor,
                format!(
                    "frame-widget-available {}",
                    json!({"output":output,"id":"tray","available":false})
                ),
            );
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let state = layout(&compositor);
            if state["animating"] == false {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "removed native member left a pending turn alive: {state}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            client.size(tools),
            (132, 44),
            "cancellation must not require remaining clients to reflow"
        );
        keys(&compositor, &[1]);
        action(&compositor, "pointer-button-up 272");
        if !destroyed {
            action(
                &compositor,
                format!(
                    "frame-widget-available {}",
                    json!({"output":output,"id":"tray","available":true})
                ),
            );
        }
    }
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn frame_layout_turn_press_uses_the_held_native_pose_and_click_release_restores_placement() {
    let _guard = headless_test_guard();
    let mut compositor = HeadlessCompositor::spawn_with_size("1280x800");
    drop(compositor.wait_for_socket());
    action(&compositor, "canvas-search-motion true");
    let output = layout(&compositor)["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut client = connect(&compositor);
    let tools = client.add(
        frame_widget_app_id(Some(&output), "tools"),
        (150, 54),
        (0, 0),
        COLOR,
    );
    action(&compositor, "layout-tool");
    let initial = widget(&layout(&compositor), "tools")["rect"].clone();
    // Materialize the old native composition before an inspector placement starts reflow.
    let _ = compositor.take_screenshot();
    command(
        &compositor,
        "place",
        json!({"kind":"widget","id":"tools"}),
        json!({"surface":"frame","edge":"left","position":0.5}),
    );
    thread::sleep(Duration::from_millis(350));
    assert_eq!(layout(&compositor)["animating"], true);
    let x = initial["x"].as_f64().unwrap() + 100.;
    let y = initial["y"].as_f64().unwrap() + 27.;
    let target = widget(&layout(&compositor), "tools")["rect"].clone();
    assert!(
        y > target["y"].as_f64().unwrap() + target["height"].as_f64().unwrap(),
        "fixture must click outside the final target"
    );
    action(&compositor, format!("pointer-motion-abs {x} {y}"));
    action(&compositor, "pointer-button-down 272");
    let pressed = layout(&compositor);
    assert_eq!(
        pressed["selected"]["target"],
        json!({"kind":"widget","id":"tools"}),
        "{pressed}"
    );
    assert_eq!(
        pressed["animating"], false,
        "press must cancel the owned pending turn"
    );
    assert!(
        client.presses().is_empty(),
        "editor must retain the native press"
    );
    action(&compositor, "pointer-button-up 272");
    let restored = layout(&compositor);
    assert_eq!(
        restored["outputs"][0]["layout"]["placements"]["widget:tools"]["edge"],
        "left"
    );
    assert_eq!(client.size(tools), (150, 54));
    keys(&compositor, &[1]);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
