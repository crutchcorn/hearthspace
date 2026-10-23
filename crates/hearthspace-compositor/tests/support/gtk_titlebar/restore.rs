use super::*;

#[test]
fn gtk_titlebar_restore_reveals_after_the_original_press_is_released() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture();
    // A canvas maximization is visible independently of the saved floating
    // position. Restoring it must reveal that position after input is released.
    action(&compositor, format!("move-window {id} 4000 4000"));
    action(&compositor, format!("maximize-window {id} true"));
    state(&compositor, &mut client, id, true, false);
    let viewport = query(&compositor, "status")["viewport"].clone();
    let serial = press(&compositor, &mut client, id, 272);
    let releases = client.released_buttons().len();
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    let restored = state(&compositor, &mut client, id, false, false);
    assert_eq!(restored["content_width"], 240);
    assert_eq!(restored["content_height"], 160);
    assert_eq!(restored["canvas_x"], 4000);
    assert_eq!(restored["canvas_y"], 4000);
    assert_eq!(client.released_buttons().len(), releases);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    release(&compositor, &mut client, 272);
    let windows = wait_windows(&compositor, |windows| {
        windows.iter().any(|window| {
            window["id"] == id
                && window["x"]
                    .as_f64()
                    .is_some_and(|x| (0.0..1000.0).contains(&x))
                && window["y"]
                    .as_f64()
                    .is_some_and(|y| (48.0..600.0).contains(&y))
        })
    });
    let restored = windows.iter().find(|window| window["id"] == id).unwrap();
    assert_eq!(restored["focused"], true);
    assert_eq!(restored["canvas_x"], 4000);
    assert_eq!(restored["canvas_y"], 4000);
    assert_eq!(client.released_buttons().len(), releases + 1);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
