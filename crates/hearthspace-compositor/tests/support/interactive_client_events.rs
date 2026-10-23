//! Ordered Wayland input observations for the interactive client fixture.
use super::super::super::{HeadlessCompositor, navigation::action};
use super::{InteractiveClient, State};
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum,
    protocol::{wl_keyboard, wl_pointer, wl_registry, wl_seat},
};
use wl_pointer::{ButtonState, Event};

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), handle, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, 1, handle, ())),
                "zxdg_decoration_manager_v1" => {
                    state.decoration_manager = Some(registry.bind(name, 1, handle, ()));
                }
                "xdg_wm_base" => {
                    state.wm_base = Some(registry.bind(name, version.min(6), handle, ()))
                }
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, version.min(7), handle, ());
                    state.pointer = Some(seat.get_pointer(handle, ()));
                    state.keyboard = Some(seat.get_keyboard(handle, ()));
                    state.seat = Some(seat);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match &event {
            wl_pointer::Event::Button {
                serial,
                state: WEnum::Value(wl_pointer::ButtonState::Pressed),
                ..
            } => state.press_serial = Some(*serial),
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(wl_pointer::ButtonState::Released),
                ..
            } => state.released_buttons.push(*button),
            _ => {}
        }
        if !matches!(event, wl_pointer::Event::Frame) {
            state.pointer_events.push(event);
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(
            event,
            wl_keyboard::Event::Enter { .. }
                | wl_keyboard::Event::Leave { .. }
                | wl_keyboard::Event::Key { .. }
        ) {
            state.keyboard_events.push(event);
        }
    }
}

pub(crate) fn assert_buttons(events: &[Event], expected: &[(u32, ButtonState)]) {
    let buttons: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::Button {
                button,
                state: WEnum::Value(state),
                ..
            } => Some((*button, *state)),
            _ => None,
        })
        .collect();
    assert_eq!(buttons, expected, "pointer events: {events:?}");
}

pub(crate) fn assert_leave(client: &mut InteractiveClient, number: usize) {
    let events = client.take_pointer_events();
    assert!(
        matches!(events.as_slice(), [Event::Leave { surface, .. }] if surface == client.surface(number)),
        "accepting the request must immediately send only leave: {events:?}",
    );
}

pub(crate) fn assert_silent(client: &mut InteractiveClient, reason: &str) {
    let events = client.take_pointer_events();
    assert!(events.is_empty(), "{reason}: {events:?}");
}

pub(crate) fn assert_reentered(client: &InteractiveClient, events: &[Event], number: usize) {
    assert!(
        events.iter().any(|event| matches!(event, Event::Enter { surface, .. } if surface == client.surface(number))),
        "pointer must enter window {number} without more motion: {events:?}",
    );
    assert!(
        events.iter().all(|event| match event {
            Event::Enter { surface, .. } => surface == client.surface(number),
            Event::Motion { .. } => true,
            _ => false,
        }),
        "completion leaked events: {events:?}"
    );
}

pub(crate) fn assert_keyboard_enter(client: &mut InteractiveClient, number: usize) {
    let events = client.take_keyboard_events();
    assert!(
        matches!(events.as_slice(), [wl_keyboard::Event::Enter { surface, .. }] if surface == client.surface(number)),
        "keyboard must enter window {number} before another content click: {events:?}",
    );
}

pub(crate) fn assert_keys(client: &mut InteractiveClient, keycode: u32) {
    let events = client.take_keyboard_events();
    assert!(
        matches!(events.as_slice(), [
            wl_keyboard::Event::Key { key: pressed, state: WEnum::Value(wl_keyboard::KeyState::Pressed), .. },
            wl_keyboard::Event::Key { key: released, state: WEnum::Value(wl_keyboard::KeyState::Released), .. },
        ] if *pressed == keycode && *released == keycode),
        "focused window must receive the key press and release: {events:?}",
    );
}

pub(crate) fn motion(compositor: &HeadlessCompositor, point: (f64, f64)) {
    action(
        compositor,
        format!("pointer-motion-abs {} {}", point.0, point.1),
    );
}

pub(crate) fn assert_content_click(
    compositor: &HeadlessCompositor,
    client: &mut InteractiveClient,
) {
    // No motion here: completion must already have restored the client focus.
    action(compositor, "pointer-button-down 272");
    action(compositor, "pointer-button-up 272");
    let events = client.take_pointer_events();
    assert_buttons(
        &events,
        &[(272, ButtonState::Pressed), (272, ButtonState::Released)],
    );
    assert!(
        events
            .iter()
            .all(|event| matches!(event, Event::Motion { .. } | Event::Button { .. })),
        "content click unexpectedly changed pointer focus: {events:?}"
    );
}

pub(crate) fn assert_keyboard_unfocused(client: &mut InteractiveClient) {
    let events = client.take_keyboard_events();
    assert!(
        events
            .iter()
            .all(|event| matches!(event, wl_keyboard::Event::Leave { .. })),
        "keyboard focus or keys escaped before every held button drained: {events:?}",
    );
}
