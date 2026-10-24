//! Retiring other records must not retire the active input sequence's owner.
use super::*;

#[rstest::rstest]
#[case::move_owner(Operation::Move)]
#[case::resize_owner(Operation::Resize)]
fn unrelated_retirement_and_replacement_preserve_owned_pointer_sequence(
    #[case] operation: Operation,
) {
    let _guard = headless_test_guard();
    let (mut compositor, mut client) = fixture();
    let mut shell = shell_panels::connect(&compositor);
    let first_osd = shell.add(
        hearthspace_common::CONTROL_OSD_APP_ID.into(),
        (120, 40),
        (0, 0),
        [0, 255, 0, 255],
    );
    let second_osd = shell.add(
        hearthspace_common::CONTROL_OSD_APP_ID.into(),
        (120, 40),
        (0, 0),
        [0, 255, 0, 255],
    );
    action(&compositor, "pan-to 25 40");
    let viewport = query(&compositor, "status")["viewport"].clone();
    let owner = window(&compositor, 1);
    let owner_id = owner["id"].clone();
    let retired_id = window(&compositor, 0)["id"].clone();
    let original = geometry(&owner);
    let (point, serial) = press(&compositor, &mut client, 1);
    operation.request(&mut client, 1, serial);
    assert_leave(&mut client, 1);
    action(&compositor, "pointer-button-down 273");

    // Remove a record below the owner, then replace it while navigation is
    // unavailable. Both operations invalidate indices without changing owner ID.
    client.unmap(0);
    client.remap(0);
    assert_eq!(window(&compositor, 0)["id"], retired_id);
    client.destroy(0);
    wait_windows(&compositor, |windows| windows.len() == 1);
    shell.destroy(first_osd);
    shell.add(
        "dev.hearthspace.LifecycleReplacement".into(),
        (120, 80),
        (0, 0),
        [0, 0, 255, 255],
    );
    let windows = wait_windows(&compositor, |windows| windows.len() == 2);
    let replacement = windows
        .iter()
        .find(|window| window["id"] != owner_id)
        .unwrap();
    assert!(replacement["id"].as_u64().unwrap() > owner_id.as_u64().unwrap());
    let replacement_id = replacement["id"].clone();
    let replacement_geometry = geometry(replacement);
    motion(&compositor, (point.0 + 30.0, point.1 + 18.0));
    assert_silent(&mut client, "unrelated teardown released pointer ownership");
    assert_eq!(window(&compositor, 1)["id"], owner_id);
    assert_eq!(window(&compositor, 1)["focused"], true);
    let completed = operation.expected(original, 30, 18);
    assert_eq!(geometry(&window(&compositor, 1)), completed);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);

    // Once the initiating button releases, geometry stops. Destroying another
    // shell surface must still leave the secondary release owned by the grab.
    action(&compositor, "pointer-button-up 272");
    shell.destroy(second_osd);
    motion(&compositor, (point.0 + 50.0, point.1 + 28.0));
    assert_silent(&mut client, "shell teardown interrupted held-release drain");
    assert_eq!(geometry(&window(&compositor, 1)), completed);
    assert!(!client.configuration(1).1);
    let windows = query(&compositor, "windows");
    assert_eq!(
        geometry(
            windows["windows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|window| window["id"] == replacement_id)
                .unwrap()
        ),
        replacement_geometry,
        "the replacement must not inherit the retired record's operation",
    );
    action(&compositor, "pointer-button-up 273");
    let events = client.take_pointer_events();
    assert_reentered(&client, &events, 1);
    assert_content_click(&compositor, &mut client);
    assert_eq!(query(&compositor, "status")["viewport"], viewport);
    action(&compositor, "quit");
    wait_for_exit(&mut compositor.child);
}
