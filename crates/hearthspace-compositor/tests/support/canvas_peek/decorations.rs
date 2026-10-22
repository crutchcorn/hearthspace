use super::*;

#[test]
fn canvas_peek_icon_uses_artwork_at_zoom_and_returns_only_when_its_whole_home_clears() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let mut client = connect(&compositor);
    let (cover, id) = normal(
        &compositor,
        &mut client,
        "peek-icon-cover",
        (400, 300),
        (800, 240),
    );
    let item = "file:7065656b";
    client.add(
        hearthspace_common::desktop_item_app_id(item),
        (100, 108),
        (0, 0),
        COLOR,
    );
    place(
        &compositor,
        json!({"kind":"item","id":item}),
        "background",
        320.0,
        300.0,
    );
    wait_item(&compositor, item, [320, 300]);
    action(&compositor, "zoom-to 0.8");
    super::super::canvas_zoom::wait_scale(&compositor, 0.8);
    action(&compositor, "pan-to 0 0");

    drag_header_to(&compositor, id, (300, 240));
    client.sync();
    assert_eq!(
        item_position(&compositor, item),
        [320, 300],
        "icons must also wait for release"
    );
    action(&compositor, "pointer-button-up 272");
    // The 100x108 native surface pads an 88x96 tile. Its centered 56px
    // artwork begins 16px into that tile. At x=252 its
    // artwork spans 268..324, exposing 32 canvas pixels left of the app.
    wait_item(&compositor, item, [252, 300]);
    assert_eq!(geometry(&window(&compositor, id)), [300, 240, 400, 300]);

    // Artwork at home ends at 392, but the full icon/label ends at 408.
    action(&compositor, format!("move-window {id} 400 240"));
    client.sync();
    assert_eq!(
        item_position(&compositor, item),
        [252, 300],
        "clear artwork alone must not restore a still-covered label"
    );
    action(&compositor, format!("move-window {id} 800 240"));
    wait_item(&compositor, item, [320, 300]);

    drag_header_to(&compositor, id, (300, 240));
    action(&compositor, "pointer-button-up 272");
    wait_item(&compositor, item, [252, 300]);
    client.destroy(cover);
    wait_windows(&compositor, |windows| windows.is_empty());
    wait_item(&compositor, item, [320, 300]);
    assert_eq!(query(&compositor, "status")["viewport"]["scale"], 0.8);
    shutdown(compositor);
}

fn control_rect(compositor: &HeadlessCompositor) -> Value {
    query(compositor, "layout-status")["outputs"][0]["widgets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|widget| widget["id"] == "wifi")
        .unwrap()["rect"]
        .clone()
}

fn wait_control(compositor: &HeadlessCompositor, expected: Value) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let actual = control_rect(compositor);
        if actual == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "control: expected {expected}, got {actual}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn canvas_peek_control_restores_but_background_layout_drop_adopts_home_without_moving_apps() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let mut client = connect(&compositor);
    let (cover, id) = normal(
        &compositor,
        &mut client,
        "peek-control-cover",
        (500, 400),
        (800, 200),
    );
    let output = query(&compositor, "layout-status")["outputs"][0]["output"]
        .as_str()
        .unwrap()
        .to_owned();
    client.add(
        hearthspace_common::frame_layout::frame_widget_app_id(Some(&output), "wifi"),
        (44, 44),
        (0, 0),
        COLOR,
    );
    let target = json!({"kind":"widget","id":"wifi"});
    place(&compositor, target.clone(), "canvas", 600.0, 300.0);
    let home = json!({"x":600.0,"y":300.0,"width":44.0,"height":44.0});
    wait_control(&compositor, home.clone());

    drag_header_to(&compositor, id, (580, 280));
    client.sync();
    assert_eq!(control_rect(&compositor), home);
    action(&compositor, "pointer-button-up 272");
    let peek = json!({"x":600.0,"y":248.0,"width":44.0,"height":44.0});
    wait_control(&compositor, peek.clone());
    assert_eq!(geometry(&window(&compositor, id)), [580, 280, 500, 400]);
    action(&compositor, format!("move-window {id} 800 200"));
    wait_control(&compositor, home.clone());

    // An ordinary app lies completely inside the forthcoming background drop.
    // It must remain fixed while the background control makes permanent room.
    let (_, ordinary) = normal(
        &compositor,
        &mut client,
        "peek-fixed-app",
        (260, 160),
        (750, 500),
    );
    place(&compositor, target, "background", 600.0, 300.0);
    wait_control(&compositor, home);
    let app_before = geometry(&window(&compositor, ordinary));
    place(
        &compositor,
        json!({"kind":"window","id":id.to_string()}),
        "background",
        580.0,
        280.0,
    );
    wait_control(&compositor, peek.clone());
    assert_eq!(
        geometry(&window(&compositor, ordinary)),
        app_before,
        "background layout placement must not rearrange ordinary apps"
    );

    client.destroy(cover);
    wait_windows(&compositor, |windows| windows.len() == 1);
    assert_eq!(
        control_rect(&compositor),
        peek,
        "the background drop's cleared space is the control's new home"
    );
    // A later temporary displacement must restore to that adopted home.
    action(&compositor, format!("move-window {ordinary} 580 230"));
    wait_control(
        &compositor,
        json!({"x":600.0,"y":198.0,"width":44.0,"height":44.0}),
    );
    action(&compositor, format!("move-window {ordinary} 750 500"));
    wait_control(&compositor, peek);
    shutdown(compositor);
}
