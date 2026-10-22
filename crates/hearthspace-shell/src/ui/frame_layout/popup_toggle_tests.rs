use super::*;
use crate::ui::{quick_panel::Kind, tray_state};

fn queued(fixture: &mut Fixture, now: Instant) -> Opening {
    fixture
        .requests
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(1, now)
}

#[test]
fn reversing_pending_toggle_preserves_panel_dismissal_and_quick_kind_reset() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    fixture.state.outputs[0].show_notifications = true;
    fixture.state.toggle_power("eDP-1");
    let old = queued(&mut fixture, now);
    fixture.state.toggle_power("eDP-1");
    old.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
    assert!(!fixture.state.outputs[0].show_power);
    assert!(fixture.requests.try_recv().is_err());

    fixture.state.outputs[0].show_controls = true;
    fixture.state.outputs[0].quick_panel = Kind::Sound;
    fixture.state.toggle_quick_panel("eDP-1", Kind::Wifi);
    let old = queued(&mut fixture, now);
    fixture.state.toggle_quick_panel("eDP-1", Kind::Wifi);
    old.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_controls);
    assert_eq!(fixture.state.outputs[0].quick_panel, Kind::Wifi);
}

#[test]
fn different_quick_kinds_replace_instead_of_reversing() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    fixture.state.toggle_quick_panel("eDP-1", Kind::Wifi);
    let old = queued(&mut fixture, now);
    fixture.state.toggle_quick_panel("eDP-1", Kind::Sound);
    let new = queued(&mut fixture, now);
    old.deliver(&mut fixture.state, now);
    new.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_controls);
    assert_eq!(fixture.state.outputs[0].quick_panel, Kind::Sound);
}

#[test]
fn set_open_and_toggle_compose_in_order_and_keep_the_newest_request_token() {
    for first_toggle in [false, true] {
        let mut fixture = Fixture::new();
        let now = Instant::now();
        assert!(frame_layout::popups::defer(
            &mut fixture.state,
            "eDP-1",
            Popup::AppsRequest {
                token: 4,
                toggle: first_toggle
            }
        ));
        let old = queued(&mut fixture, now);
        assert!(frame_layout::popups::defer(
            &mut fixture.state,
            "eDP-1",
            Popup::AppsRequest {
                token: 5,
                toggle: !first_toggle
            }
        ));
        old.deliver(&mut fixture.state, now);
        if first_toggle {
            queued(&mut fixture, now).deliver(&mut fixture.state, now);
            assert!(fixture.state.outputs[0].show_search);
            assert_eq!(fixture.state.outputs[0].search_request_token, Some(5));
        } else {
            assert!(fixture.requests.try_recv().is_err());
            assert!(!fixture.state.outputs[0].show_search);
            assert_eq!(fixture.state.outputs[0].search_request_token, None);
        }
    }
}

#[test]
fn repeated_tray_requests_stay_set_open_and_preserve_newest_item_identity() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    fixture.state.tray.snapshot.items = vec![
        crate::ui::tray_tests::item(1),
        crate::ui::tray_tests::item(2),
    ];
    let first = fixture.state.tray.snapshot.items[0].key.clone();
    let second = fixture.state.tray.snapshot.items[1].key.clone();
    tray_state::open_menu(&mut fixture.state, "eDP-1", &first);
    let old = queued(&mut fixture, now);
    tray_state::open_menu(&mut fixture.state, "eDP-1", &first);
    let repeated = queued(&mut fixture, now);
    tray_state::open_menu(&mut fixture.state, "eDP-1", &second);
    let newest = queued(&mut fixture, now);
    old.deliver(&mut fixture.state, now);
    repeated.deliver(&mut fixture.state, now);
    newest.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].tray.open);
    assert_eq!(fixture.state.outputs[0].tray.item, Some(second));
}
