//! Deferred keyboard recovery survives owner remapping and session input reset.
use super::super::session_lock::lock_client::LockClient;
use super::*;
use wayland_client::protocol::wl_keyboard;

#[rstest::rstest]
#[case::move_owner(Operation::Move)]
#[case::resize_owner(Operation::Resize)]
fn csd_remapped_owner_recovers_keyboard_after_all_buttons_drain(#[case] operation: Operation) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let owner_id = window(&compositor, 0)["id"].clone();
    let (_, serial) = press(&compositor, &mut client, 0);
    client.take_keyboard_events();
    operation.request(&mut client, 0, serial);
    assert_leave(&mut client, 0);
    action(&compositor, "pointer-button-down 273");
    client.unmap(0);
    wait_windows(&compositor, |windows| windows.len() == 1);
    assert_keyboard_unfocused(&mut client);
    // No motion or release while unmapped: only lifecycle handling can retain
    // the lost focus across this remap of the same compositor window.
    client.remap(0);
    wait_windows(&compositor, |windows| windows.len() == 2);
    assert_eq!(window(&compositor, 0)["id"], owner_id);
    let original = geometry(&window(&compositor, 0));
    motion(&compositor, center(&window(&compositor, 1)));
    keys(&compositor, &[30]);
    assert_keyboard_unfocused(&mut client);
    action(&compositor, "pointer-button-up 272");
    assert_silent(
        &mut client,
        "primary release escaped the remapped owner's grab",
    );
    keys(&compositor, &[30]);
    assert_keyboard_unfocused(&mut client);
    action(&compositor, "pointer-button-up 273");
    let events = client.take_pointer_events();
    assert_reentered(&client, &events, 1);
    // Automatic focus follows stacking, independently of pointer hover. The
    // remapped owner is still topmost and must receive keys without a click.
    assert_keyboard_enter(&mut client, 0);
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
    assert_eq!(geometry(&window(&compositor, 0)), original);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[derive(Clone, Copy, Debug)]
enum ResetOwner {
    Unmapped,
    Destroyed,
    Remapped,
}

#[rstest::rstest]
#[case::unmapped_move(Operation::Move, ResetOwner::Unmapped)]
#[case::unmapped_resize(Operation::Resize, ResetOwner::Unmapped)]
#[case::destroyed_move(Operation::Move, ResetOwner::Destroyed)]
#[case::destroyed_resize(Operation::Resize, ResetOwner::Destroyed)]
#[case::remapped_move(Operation::Move, ResetOwner::Remapped)]
#[case::remapped_resize(Operation::Resize, ResetOwner::Remapped)]
fn csd_session_reset_recovers_retired_owner_keyboard(
    #[case] operation: Operation,
    #[case] owner: ResetOwner,
) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let (_, serial) = press(&compositor, &mut client, 0);
    client.take_keyboard_events();
    operation.request(&mut client, 0, serial);
    assert_leave(&mut client, 0);
    action(&compositor, "pointer-button-down 273");
    if matches!(owner, ResetOwner::Destroyed) {
        client.destroy(0);
    } else {
        client.unmap(0);
    }
    wait_windows(&compositor, |windows| windows.len() == 1);
    if matches!(owner, ResetOwner::Remapped) {
        client.remap(0);
        wait_windows(&compositor, |windows| windows.len() == 2);
    }
    motion(&compositor, center(&window(&compositor, 1)));
    // Reset must release this unfocused key before restoring the successor;
    // the successor must not receive an unmatched release from the reset.
    action(&compositor, "key-down 30");
    assert_keyboard_unfocused(&mut client);
    action(&compositor, "session-resume");
    let target = if matches!(owner, ResetOwner::Remapped) {
        0
    } else {
        1
    };
    assert_keyboard_enter(&mut client, target);
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
    assert_buttons(&client.take_pointer_events(), &[]);
    // A physical release arriving after reset is discarded by button-source
    // tracking; it cannot be relied on to complete the deferred focus handoff.
    action(&compositor, "pointer-button-up 272");
    action(&compositor, "pointer-button-up 273");
    assert_silent(&mut client, "late releases leaked after session reset");
    assert!(client.take_keyboard_events().is_empty());
    if matches!(owner, ResetOwner::Remapped) {
        action(
            &compositor,
            format!("focus-window {}", window(&compositor, 1)["id"]),
        );
        let events = client.take_keyboard_events();
        assert!(
            matches!(events.as_slice(), [
                wl_keyboard::Event::Leave { surface: old, .. },
                wl_keyboard::Event::Enter { surface: new, .. },
            ] if old == client.surface(0) && new == client.surface(1)),
            "explicit selection must establish the newer focus: {events:?}",
        );
    }
    // Recovery is consumed: a later reset must preserve any newer focus.
    action(&compositor, "session-resume");
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[rstest::rstest]
#[case::move_owner(Operation::Move)]
#[case::resize_owner(Operation::Resize)]
fn csd_session_reset_preserves_existing_keyboard_focus(#[case] operation: Operation) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let (_, serial) = press(&compositor, &mut client, 0);
    client.take_keyboard_events();
    operation.request(&mut client, 0, serial);
    assert_leave(&mut client, 0);
    // An unrelated remap places window 1 last in the stack. It neither retires
    // this grab's owner nor permits reset to replace the owner's live focus.
    client.unmap(1);
    client.remap(1);
    action(&compositor, "session-resume");
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
    assert_eq!(window(&compositor, 0)["focused"], true);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}

#[test]
fn csd_retired_owner_reset_keeps_locked_session_isolated() {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let (_, serial) = press(&compositor, &mut client, 0);
    client.request_move(0, serial);
    assert_leave(&mut client, 0);
    client.unmap(0);
    wait_windows(&compositor, |windows| windows.len() == 1);
    let socket = compositor
        .runtime_dir
        .path()
        .join(format!("hearthspace-test-{}", std::process::id()));
    let mut locker = LockClient::connect(&socket);
    locker.wait_locked();
    client.take_keyboard_events();
    action(&compositor, "session-resume");
    assert_eq!(query(&compositor, "status")["locked"], true);
    assert!(client.take_keyboard_events().is_empty());
    assert!(
        compositor
            .send_text_command("key-down 30")
            .starts_with("err session is locked")
    );
    locker.unlock();
    assert_keyboard_enter(&mut client, 1);
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
