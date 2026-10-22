use super::*;

#[test]
fn canvas_peek_focus_fullscreen_and_search_preserve_the_underlying_canvas() {
    let _guard = headless_test_guard();
    let compositor = fixture();
    let (mut client, [active, passive]) = interactive(&compositor);
    let mut desktop = connect(&compositor);
    let item = "file:70726573656e746174696f6e";
    desktop.add(
        hearthspace_common::desktop_item_app_id(item),
        (100, 108),
        (0, 0),
        COLOR,
    );
    place(
        &compositor,
        json!({"kind":"item","id":item}),
        "background",
        500.0,
        500.0,
    );
    wait_item(&compositor, item, [500, 500]);
    let active_before = geometry(&window(&compositor, active));
    let passive_before = geometry(&window(&compositor, passive));
    let assert_underlying = || {
        assert_eq!(geometry(&window(&compositor, passive)), passive_before);
        assert_eq!(item_position(&compositor, item), [500, 500]);
    };

    action(&compositor, format!("focus-window {active}"));
    client.sync();
    assert_underlying();
    keys(&compositor, &[125, 42, 33]); // Super+Shift+F enters focused view.
    client.sync();
    assert_eq!(window(&compositor, active)["focused_view"], true);
    assert_underlying();
    // Fully restore: leaving only focused view would keep canvas maximization,
    // and that separate operation is allowed to displace covered work.
    action(&compositor, format!("restore-window {active}"));
    client.sync();
    assert_eq!(geometry(&window(&compositor, active)), active_before);
    assert_underlying();

    action(&compositor, format!("fullscreen-window {active} true"));
    client.sync();
    assert_eq!(window(&compositor, active)["fullscreen"], true);
    assert_underlying();
    action(&compositor, format!("fullscreen-window {active} false"));
    client.sync();
    assert_eq!(geometry(&window(&compositor, active)), active_before);
    assert_underlying();

    action(&compositor, "canvas-search-open");
    let session = query(&compositor, "canvas-search-status")["session"]
        .as_u64()
        .unwrap();
    action(
        &compositor,
        format!(
            "canvas-search-update {}",
            json!({
                "session":session,"query":"Interactive","handwriting":false,
            })
        ),
    );
    let search = query(&compositor, "canvas-search-status");
    assert_eq!(search["active"], true);
    assert_eq!(search["results"].as_array().unwrap().len(), 2);
    assert_eq!(geometry(&window(&compositor, active)), active_before);
    assert_underlying();
    action(&compositor, format!("canvas-search-close {session}"));
    client.sync();
    assert_eq!(query(&compositor, "canvas-search-status")["active"], false);
    assert_eq!(geometry(&window(&compositor, active)), active_before);
    assert_underlying();
    shutdown(compositor);
}
