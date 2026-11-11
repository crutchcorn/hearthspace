//! Chrome-style subsurface controls must not steal keyboard focus.
use super::{
    interactive::client::InteractiveClient,
    navigation::{action, wait_windows},
    *,
};
use wayland_client::{
    WEnum,
    protocol::wl_pointer::{ButtonState, Event},
};

fn click_subsurface() {
    let _guard = headless_test_guard();
    let mut c = HeadlessCompositor::spawn_with_size("1280x800");
    drop(c.wait_for_socket());
    let socket = c
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut client = InteractiveClient::connect(&socket);
    let windows = wait_windows(&c, |w| w.len() == 2);
    let id = windows
        .iter()
        .find(|w| w["title"] == "Interactive fixture 0")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    action(&c, format!("move-window {id} 100 150"));
    action(&c, "pointer-motion-abs 120 170");
    action(&c, "pointer-button-down 272");
    client.press_serial();
    client.open_subsurface();
    action(&c, "pointer-button-up 272");
    client.take_pointer_events();
    action(&c, "pointer-motion-abs 200 210");
    let events = client.take_pointer_events();
    assert!(events.iter().any(|e| matches!(e, Event::Enter {surface, surface_x, surface_y,..}
        if surface == client.subsurface_surface() && (*surface_x-60.).abs()<1. && (*surface_y-30.).abs()<1.)), "subsurface enter: {events:?}");
    client.take_keyboard_events();
    action(&c, "pointer-button-down 272");
    action(&c, "pointer-button-up 272");
    let keyboard = client.take_keyboard_events();
    assert!(
        keyboard.is_empty(),
        "subsurface click must retain owning toplevel keyboard focus: {keyboard:?}"
    );
    let events = client.take_pointer_events();
    let buttons: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Button {
                state: WEnum::Value(s),
                ..
            } => Some(*s),
            _ => None,
        })
        .collect();
    assert_eq!(
        buttons,
        [ButtonState::Pressed, ButtonState::Released],
        "{events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, Event::Leave { .. })),
        "click must not leave subsurface: {events:?}"
    );
    action(&c, "key-down 30");
    action(&c, "key-up 30");
    let keys = client.take_keyboard_events();
    assert!(
        matches!(
            keys.as_slice(),
            [
                wayland_client::protocol::wl_keyboard::Event::Key {
                    key: 30,
                    state: WEnum::Value(wayland_client::protocol::wl_keyboard::KeyState::Pressed),
                    ..
                },
                wayland_client::protocol::wl_keyboard::Event::Key {
                    key: 30,
                    state: WEnum::Value(wayland_client::protocol::wl_keyboard::KeyState::Released),
                    ..
                }
            ]
        ),
        "owning window must still receive typing: {keys:?}"
    );
}

#[test]
fn dropdown_subsurface_click_keeps_keyboard_focus_on_its_toplevel() {
    click_subsurface();
}
