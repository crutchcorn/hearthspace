//! Client-driven focused presentation revokes Pan without releasing its grab.
use super::*;

fn window(compositor: &HeadlessCompositor, id: u64) -> Value {
    query(compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["id"] == id)
        .unwrap()
        .clone()
}

#[rstest::rstest]
#[case::selected_pan(false)]
#[case::temporary_meta(true)]
fn canvas_pan_client_maximize_stops_drag_drains_buttons_and_allows_reselection(
    #[case] temporary: bool,
) {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    let id = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Interactive fixture 1")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    action(&compositor, format!("focus-window {id}"));
    if temporary {
        action(&compositor, "key-down 125");
    } else {
        action(&compositor, "canvas-tool pan");
    }
    start(&compositor, (500., 500.));
    motion(&compositor, (550., 550.));
    action(&compositor, "pointer-button-down 273");
    let stopped = viewport(&compositor);
    // This is a real xdg_toplevel.set_maximized, without a navigation command
    // that would reject the active grab before exercising presentation entry.
    client.gtk_request_maximized(1, true);
    assert_eq!(window(&compositor, id)["focused_view"], true);
    assert_eq!(tool(&compositor), Value::Null);
    motion(&compositor, (650., 600.));
    assert_eq!(viewport(&compositor), stopped);
    assert_silent(
        &mut client,
        "focused presentation stole the held pan sequence",
    );
    if temporary {
        action(&compositor, "key-up 125");
    }

    // Selecting Pan again exits focused view even while the cancelled sequence
    // drains. It cannot revive the old drag or deliver its releases to the app.
    action(&compositor, "canvas-tool pan");
    client.sync();
    assert_eq!(window(&compositor, id)["focused_view"], false);
    assert_eq!(tool(&compositor), "pan");
    motion(&compositor, (20., 600.));
    finish(&compositor);
    motion(&compositor, (40., 620.));
    assert_eq!(viewport(&compositor), stopped);
    assert_silent(
        &mut client,
        "reselection released capture before every button drained",
    );
    action(&compositor, "pointer-button-up 273");
    assert_buttons(&client.take_pointer_events(), &[]);

    start(&compositor, (20., 600.));
    motion(&compositor, (40., 620.));
    finish(&compositor);
    assert_eq!(
        offset(&compositor),
        [
            stopped["x"].as_i64().unwrap() - 20,
            stopped["y"].as_i64().unwrap() - 20
        ]
    );
    assert_buttons(&client.take_pointer_events(), &[]);
}

#[rstest::rstest]
#[case::selected_pan(false)]
#[case::temporary_meta(true)]
fn canvas_pan_outside_palette_press_drains_when_restored_focus_activates_pending_maximize(
    #[case] temporary: bool,
) {
    let _guard = headless_test_guard();
    let compositor = fixture(false);
    let mut client = clients(&compositor);
    let id = query(&compositor, "windows")["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["title"] == "Interactive fixture 1")
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    if !temporary {
        action(&compositor, "canvas-tool pan");
    }
    let mut shell = crate::shell_panels::connect(&compositor);
    let panel = shell.add(
        hearthspace_common::TRAY_MENU_APP_ID.into(),
        (250, 150),
        (0, 0),
        [40, 180, 100, 255],
    );
    assert_eq!(shell.focus(), Some(panel));
    client.gtk_request_maximized(1, true);
    assert_eq!(window(&compositor, id)["maximized"], true);
    assert_eq!(window(&compositor, id)["focused_view"], false);
    assert_eq!(tool(&compositor), if temporary { "select" } else { "pan" });
    // The canvas-maximized client leaves a 24px margin. The outside press
    // starts on blank canvas; restoring focus expands the client over it.
    motion(&compositor, (20., 500.));
    if temporary {
        action(&compositor, "key-down 125");
    }
    client.take_pointer_events();
    let stopped = viewport(&compositor);
    action(&compositor, "pointer-button-down 272");
    assert!(shell.closed().contains(&panel));
    assert_eq!(window(&compositor, id)["focused"], true);
    assert_eq!(window(&compositor, id)["focused_view"], true);
    assert_eq!(tool(&compositor), Value::Null);
    action(&compositor, "pointer-button-down 273");
    motion(&compositor, (70., 550.));
    finish(&compositor);
    motion(&compositor, (90., 570.));
    assert_eq!(
        viewport(&compositor),
        stopped,
        "focus restoration restarted Pan"
    );
    assert_silent(
        &mut client,
        "outside palette press became client input before draining",
    );
    action(&compositor, "pointer-button-up 273");
    if temporary {
        action(&compositor, "key-up 125");
    }
    assert_buttons(&client.take_pointer_events(), &[]);
    // The next complete click belongs to the newly focused app.
    start(&compositor, (90., 570.));
    finish(&compositor);
    assert_buttons(
        &client.take_pointer_events(),
        &[(272, ButtonState::Pressed), (272, ButtonState::Released)],
    );
    client.take_keyboard_events();
    keys(&compositor, &[30]);
    assert_keys(&mut client, 30);
}
