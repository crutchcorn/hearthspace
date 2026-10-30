//! Shortcut outcomes and complete Wayland key pairs, including cancelled drags.
use super::*;
use wayland_client::{WEnum, protocol::wl_keyboard};

fn key_events(client: &mut InteractiveClient) -> Vec<(u32, wl_keyboard::KeyState)> {
    client
        .take_keyboard_events()
        .into_iter()
        .filter_map(|event| match event {
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(state),
                ..
            } => Some((key, state)),
            _ => None,
        })
        .collect()
}

#[test]
fn canvas_shortcut_repeats_and_meta_first_release_never_leak_the_tool_key() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    for meta in [125, 126] {
        action(&compositor, "canvas-tool select");
        action(&compositor, format!("key-down {meta}"));
        action(&compositor, "key-down 35");
        action(&compositor, "key-down 35");
        assert_eq!(tool(&compositor), "pan");
        action(&compositor, format!("key-up {meta}"));
        action(&compositor, "key-down 35");
        action(&compositor, "key-up 35");
        assert_eq!(tool(&compositor), "pan");
        assert_eq!(
            key_events(&mut client),
            [
                (meta, wl_keyboard::KeyState::Pressed),
                (meta, wl_keyboard::KeyState::Released),
            ]
        );
        keys(&compositor, &[35]);
        assert_keys(&mut client, 35);
    }
}

#[test]
fn app_owned_h_release_stays_paired_when_meta_is_pressed_during_key_repeat() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    for command in [
        "key-down 35",
        "key-down 125",
        "key-down 35",
        "key-up 125",
        "key-down 35",
        "key-up 35",
    ] {
        action(&compositor, command);
    }
    assert_eq!(tool(&compositor), "select");
    assert_eq!(
        key_events(&mut client),
        [
            (35, wl_keyboard::KeyState::Pressed),
            (125, wl_keyboard::KeyState::Pressed),
            (125, wl_keyboard::KeyState::Released),
            (35, wl_keyboard::KeyState::Released),
        ]
    );
}

#[test]
fn canvas_shortcuts_require_meta_alone_and_preserve_client_key_pairs() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    for code in [35, 47, 53, 37, 34, 30, 24] {
        keys(&compositor, &[code]);
        assert_keys(&mut client, code);
    }
    for modifiers in [&[29][..], &[125, 29], &[125, 56], &[125, 42]] {
        for code in [35, 47, 37, 34, 30, 24] {
            let mut chord = modifiers.to_vec();
            chord.push(code);
            keys(&compositor, &chord);
            let expected: Vec<_> = chord
                .iter()
                .map(|code| (*code, wl_keyboard::KeyState::Pressed))
                .chain(
                    chord
                        .iter()
                        .rev()
                        .map(|code| (*code, wl_keyboard::KeyState::Released)),
                )
                .collect();
            assert_eq!(key_events(&mut client), expected, "chord {chord:?}");
            assert_eq!(tool(&compositor), "select");
            assert_eq!(query(&compositor, "canvas-search-status")["active"], false);
        }
    }
}

#[test]
fn canvas_shortcuts_select_from_app_focus_and_caps_lock_does_not_disable_them() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    keys(&compositor, &[58]); // Caps Lock does not add Shift to the chord.
    client.take_keyboard_events();
    for (code, selected) in [(35, "pan"), (47, "select"), (34, "group"), (30, "layout")] {
        keys(&compositor, &[125, code]);
        assert_eq!(tool(&compositor), selected);
        keys(&compositor, &[125, code]);
        assert_eq!(tool(&compositor), selected, "selection must be idempotent");
        assert!(
            key_events(&mut client).iter().all(|(key, _)| *key != code),
            "canvas key leaked to the app"
        );
    }
    keys(&compositor, &[125, 47]);
    assert_eq!(tool(&compositor), "select");
    keys(&compositor, &[58]);
}

#[test]
fn meta_tool_change_cancels_held_pan_without_releasing_its_pointer_sequence() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    keys(&compositor, &[125, 35]);
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    action(&compositor, "pointer-button-down 273");
    keys(&compositor, &[125, 47]);
    assert_eq!(tool(&compositor), "select");
    motion(&compositor, (600., 600.));
    finish(&compositor);
    motion(&compositor, (650., 650.));
    assert_eq!(offset(&compositor), [-50, -50]);
    assert_buttons(&client.take_pointer_events(), &[]);
    action(&compositor, "pointer-button-up 273");
    assert_buttons(&client.take_pointer_events(), &[]);
    keys(&compositor, &[125, 35]);
    start(&compositor, (650., 650.));
    motion(&compositor, (670., 670.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [-70, -70]);
}

#[test]
fn canvas_shortcuts_do_not_take_over_a_client_move_or_window_switcher() {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    start(&compositor, (140., 140.));
    let serial = client.press_serial();
    client.request_move(0, serial);
    keys(&compositor, &[125, 35]);
    assert_eq!(tool(&compositor), "select");
    motion(&compositor, (190., 190.));
    finish(&compositor);
    assert_eq!(offset(&compositor), [0, 0]);
    action(&compositor, "key-down 56");
    keys(&compositor, &[15]);
    keys(&compositor, &[125, 35]);
    assert_eq!(tool(&compositor), "select");
    assert_eq!(
        query(&compositor, "status")["window_switcher"]["active"],
        true
    );
    keys(&compositor, &[1]);
    action(&compositor, "key-up 56");
}

#[test]
fn ordinary_text_and_shifted_letters_still_reach_a_real_editor() {
    use crate::navigation::{Editors, wait_windows};
    use crate::navigation_client::{editor, wait_state};

    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut editors = Editors(Vec::new());
    let (id, state) = editor(&compositor, &mut editors, 0);
    for chord in [
        &[35][..],
        &[42, 35],
        &[47],
        &[42, 47],
        &[53],
        &[37],
        &[34],
        &[30],
        &[24],
    ] {
        keys(&compositor, chord);
    }
    wait_state(&state, |value| value["text"] == "hHvV/kgao");
    keys(&compositor, &[125, 35]);
    keys(&compositor, &[125, 47]);
    keys(&compositor, &[48]);
    wait_state(&state, |value| value["text"] == "hHvV/kgaob");
    wait_windows(&compositor, |windows| {
        windows
            .iter()
            .any(|window| window["id"] == id && window["focused"] == true)
    });
}
