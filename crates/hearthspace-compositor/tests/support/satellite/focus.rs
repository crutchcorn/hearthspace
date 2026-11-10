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
