use super::super::remote_input::client::RemoteClient;
use super::*;

fn claimed_sequence(resize: bool) {
    let (mut compositor, mut client, id) = fixture();
    let original = window(&compositor, id);
    let serial = press(&compositor, &mut client, id, 272);
    if resize {
        client.request_resize(0, serial);
    } else {
        client.request_move(0, serial);
    }
    action(&compositor, "pointer-motion-rel 20 10");
    release(&compositor, &mut client, 272);
    let claimed = state(&compositor, &mut client, id, false, false);
    let field = if resize { "content_width" } else { "canvas_x" };
    assert_eq!(
        claimed[field].as_i64().unwrap(),
        original[field].as_i64().unwrap() + 20
    );
    // Crucially this is the FIRST GTK request: an earlier rejected request must
    // not consume the token and hide a replay after the operation finishes.
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    let unchanged = state(&compositor, &mut client, id, false, false);
    for field in ["canvas_x", "canvas_y", "content_width", "content_height"] {
        assert_eq!(unchanged[field], claimed[field]);
    }
    double_click(&compositor, &mut client, id);
    state(&compositor, &mut client, id, true, false);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn gtk_titlebar_move_claim_remains_consumed_after_release() {
    let _guard = headless_test_guard();
    claimed_sequence(false);
}

#[test]
fn gtk_titlebar_resize_claim_remains_consumed_after_release() {
    let _guard = headless_test_guard();
    claimed_sequence(true);
}

#[test]
fn gtk_titlebar_virtual_pointer_cleanup_cannot_complete_a_click() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client, id) = fixture_with_remote(true);
    let mut remote = RemoteClient::connect(&socket(&compositor));
    let place = |remote: &mut RemoteClient| {
        let window = window(&compositor, id);
        remote.motion(
            window["x"].as_u64().unwrap() as u32 + 40,
            window["y"].as_u64().unwrap() as u32 + 20,
            1280,
            800,
        );
    };
    place(&mut remote);
    remote.button(true);
    let serial = client.press_serial();
    remote.button(false);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, true, false);
    client.gtk_request_maximized(0, false);

    place(&mut remote);
    remote.button(true);
    let serial = client.press_serial();
    let releases = client.released_buttons().len();
    remote.destroy_pointer();
    assert_eq!(&client.released_buttons()[releases..], &[272]);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);

    let mut remote = RemoteClient::connect(&socket(&compositor));
    place(&mut remote);
    remote.button(true);
    let serial = client.press_serial();
    let releases = client.released_buttons().len();
    drop(remote);
    let deadline = Instant::now() + Duration::from_secs(5);
    while client.released_buttons().len() == releases {
        assert!(
            Instant::now() < deadline,
            "disconnected pointer retained its button"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(&client.released_buttons()[releases..], &[272]);
    client.gtk_gesture(0, serial, Gesture::DoubleClick);
    state(&compositor, &mut client, id, false, false);
    double_click(&compositor, &mut client, id);
    state(&compositor, &mut client, id, true, false);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
