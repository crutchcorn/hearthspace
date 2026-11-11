//! WM_TAKE_FOCUS must survive host keyboard re-entry, not just initial mapping.
use super::*;

fn wait_take_focus(client: &WindowClient) {
    let atom = |name: &[u8]| {
        client
            .connection
            .intern_atom(false, name)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let protocols = atom(b"WM_PROTOCOLS");
    let take = atom(b"WM_TAKE_FOCUS");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        while let Some(event) = client.connection.poll_for_event().unwrap() {
            if let Event::ClientMessage(event) = event
                && event.window == client.window
                && event.type_ == protocols
                && event.data.as_data32()[0] == take
            {
                assert_eq!(event.format, 32);
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "globally-active X11 client received no WM_TAKE_FOCUS"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

// Compositor acknowledgments and a queued WM_TAKE_FOCUS message do not
// establish that the current asynchronous Wayland -> X11 transfer has finished.
fn wait_input_focus(client: &WindowClient) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let focus = client
            .connection
            .get_input_focus()
            .unwrap()
            .reply()
            .unwrap()
            .focus;
        if focus == client.window {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "X keyboard focus did not reach {:?}; still on {focus:?}",
            client.window
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn satellite_take_focus_is_sent_when_keyboard_returns_to_an_existing_window() {
    let _guard = headless_test_guard();
    let (compositor, display) = fixture();
    let a = WindowClient::new_with_focus(&compositor, &display, "Take focus A", 0xdc143c, true);
    let b = WindowClient::new(&compositor, &display, "Ordinary B", 0x204080);
    action(&compositor, format!("focus-window {}", b.id));
    a.drain(&mut Vec::new());
    action(&compositor, format!("focus-window {}", a.id));
    wait_take_focus(&a);
    action(&compositor, format!("focus-window {}", b.id));
    action(&compositor, format!("focus-window {}", a.id));
    wait_take_focus(&a);
}

#[test]
fn satellite_locally_active_fullscreen_client_receives_keys_after_focus_returns() {
    let _guard = headless_test_guard();
    let (compositor, display) = fixture();
    let game = WindowClient::new_with_input_model(
        &compositor,
        &display,
        "Locally active game",
        0xdc143c,
        true,
        true,
    );
    let other = WindowClient::new(&compositor, &display, "Other app", 0x204080);
    action(&compositor, format!("fullscreen-window {} true", game.id));
    for _ in 0..2 {
        action(&compositor, format!("focus-window {}", other.id));
        wait_input_focus(&other);
        game.drain(&mut Vec::new());
        other.drain(&mut Vec::new());
        action(&compositor, format!("focus-window {}", game.id));
        // This client intentionally does not set X focus in response: like a
        // locally-active game, it asks the WM for direct focus as well.
        wait_take_focus(&game);
        wait_input_focus(&game);
        assert_eq!(
            game.connection
                .get_input_focus()
                .unwrap()
                .reply()
                .unwrap()
                .focus,
            game.window,
            "fullscreen game retained another application's X keyboard focus"
        );
        batch(&compositor, "key-down 30\nkey-up 30\n");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut keys = Vec::new();
        while keys.len() < 2 {
            while let Some(event) = game.connection.poll_for_event().unwrap() {
                match event {
                    Event::KeyPress(event) => keys.push((true, event)),
                    Event::KeyRelease(event) => keys.push((false, event)),
                    Event::Error(error) => panic!("X11 error: {error:?}"),
                    _ => {}
                }
            }
            assert!(
                Instant::now() < deadline,
                "fullscreen game received no balanced key: {keys:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(keys.len(), 2);
        assert!(keys[0].0 && !keys[1].0);
        for (_, event) in keys {
            assert_eq!(event.event, game.window);
            assert_eq!(event.detail, 38);
            assert_eq!(event.response_type & 0x80, 0);
        }
        while let Some(event) = other.connection.poll_for_event().unwrap() {
            assert!(
                !matches!(event, Event::KeyPress(_) | Event::KeyRelease(_)),
                "game input leaked to the other application: {event:?}"
            );
        }
        assert!(
            query(&compositor, "status")["windows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|window| window["id"] == game.id && window["fullscreen"] == true)
        );
    }
}
