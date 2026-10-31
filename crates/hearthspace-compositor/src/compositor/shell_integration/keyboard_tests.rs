//! Raw control keys use the real App keyboard policy and Wayland event delivery.
use hearthspace_ipc::command::ShellCommand;
use wayland_client::protocol::wl_keyboard::KeyState::{Pressed, Released};

mod fixture;
use fixture::Fixture;

const A: u32 = 30;
const META: u32 = 125;
const RIGHT: u32 = 106;

#[test]
fn ordinary_control_keys_deliver_exact_phases_without_requesting_redraw() {
    let mut fixture = Fixture::new();

    fixture.action(ShellCommand::KeyDown(A));
    assert!(!fixture.app.needs_redraw);
    fixture.sync();
    assert_eq!(fixture.take_keys(), [(A, Pressed)]);
    assert!(!fixture.app.needs_redraw);

    fixture.action(ShellCommand::KeyUp(A));
    assert!(!fixture.app.needs_redraw);
    fixture.sync();
    assert_eq!(fixture.take_keys(), [(A, Released)]);
    assert!(!fixture.app.needs_redraw);
}

#[test]
fn ordinary_control_keys_preserve_an_already_pending_redraw() {
    let mut fixture = Fixture::new();
    fixture.app.request_redraw();

    fixture.action(ShellCommand::KeyDown(A));
    assert!(fixture.app.needs_redraw);
    fixture.action(ShellCommand::KeyUp(A));
    assert!(fixture.app.needs_redraw);
    fixture.sync();
    assert_eq!(fixture.take_keys(), [(A, Pressed), (A, Released)]);
    assert!(fixture.app.needs_redraw);
}

#[test]
fn control_shortcut_redraws_and_owns_release_after_modifier_and_focus_changes() {
    let mut fixture = Fixture::new();
    let next_surface = fixture.surface();

    fixture.action(ShellCommand::KeyDown(META));
    fixture.sync();
    assert_eq!(fixture.take_keys(), [(META, Pressed)]);
    assert!(fixture.app.keyboard.modifier_state().logo);
    // Isolate the shortcut's visual change from any modifier/cursor redraw.
    fixture.app.needs_redraw = false;
    let start = fixture.app.viewport_offset;
    let step = fixture.app.horizontal_pan_step();

    fixture.action(ShellCommand::KeyDown(RIGHT));
    assert!(fixture.app.needs_redraw);
    let target = fixture.app.viewport_animation.as_ref().unwrap().to_offset;
    assert_eq!((target.x, target.y), (start.x + step, start.y));
    fixture.sync();
    assert!(fixture.take_keys().is_empty());

    // Release Meta first, then focus another live surface while Right is held.
    fixture.action(ShellCommand::KeyUp(META));
    fixture.sync();
    assert_eq!(fixture.take_keys(), [(META, Released)]);
    assert!(!fixture.app.keyboard.modifier_state().logo);
    fixture.focus(&next_surface);
    assert!(fixture.events.enter_keys.is_empty());
    assert_eq!(fixture.events.depressed_modifiers, Some(0));

    fixture.action(ShellCommand::KeyDown(RIGHT));
    fixture.action(ShellCommand::KeyUp(RIGHT));
    fixture.sync();
    assert!(fixture.take_keys().is_empty());

    // Ownership drains on release: a fresh ordinary Right pair reaches the new focus.
    fixture.action(ShellCommand::KeyDown(RIGHT));
    fixture.action(ShellCommand::KeyUp(RIGHT));
    fixture.sync();
    assert_eq!(fixture.take_keys(), [(RIGHT, Pressed), (RIGHT, Released)]);
}
