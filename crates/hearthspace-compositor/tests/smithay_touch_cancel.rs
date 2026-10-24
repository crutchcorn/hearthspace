//! Regressions for the vendored Smithay cancellation patch using real wl_touch.
#[path = "support/smithay_touch_cancel.rs"]
mod support;

use support::{Event, Fixture};

#[test]
fn framed_contact_cancels_and_releases_implicit_grab() {
    let mut f = Fixture::new();
    let first = f.connect();
    let next = f.connect();
    f.down(first, 0, 0);
    f.frame();
    assert!(f.touch.is_grabbed());
    f.assert_events(first, &[f.down_event(first, 0, 0), Event::Frame]);

    f.cancel();
    assert!(!f.touch.is_grabbed());
    f.frame();
    f.cancel();
    f.motion(0);
    f.up(0);
    f.frame();
    f.assert_events(first, &[Event::Cancel]);

    // Reuse the slot on a different client; the old TouchDownGrab must be gone.
    f.down(next, 1, 0);
    f.frame();
    f.up(0);
    f.frame();
    assert!(!f.touch.is_grabbed());
    f.assert_events(
        next,
        &[
            f.down_event(next, 1, 0),
            Event::Frame,
            Event::Up(0),
            Event::Frame,
        ],
    );
    f.assert_events(first, &[]);
}

#[test]
fn framed_surfaces_of_one_client_receive_one_cancel() {
    let mut f = Fixture::new();
    let client = f.connect();
    f.down(client, 0, 0);
    // Allow independent surface targets instead of TouchDownGrab's pinned focus.
    f.touch.unset_grab(&mut f.server);
    f.down(client, 1, 1);
    f.frame();
    f.assert_events(
        client,
        &[
            f.down_event(client, 0, 0),
            f.down_event(client, 1, 1),
            Event::Frame,
        ],
    );

    f.cancel();
    f.frame();
    f.assert_events(client, &[Event::Cancel]);
}

#[test]
fn pending_contact_does_not_hide_framed_contact_on_another_client() {
    let mut f = Fixture::new();
    let framed = f.connect();
    let pending = f.connect();
    f.down(framed, 0, 0);
    f.frame();
    f.assert_events(framed, &[f.down_event(framed, 0, 0), Event::Frame]);
    f.touch.unset_grab(&mut f.server);
    f.down(pending, 0, 1);

    f.cancel();
    f.frame();
    f.assert_events(framed, &[Event::Cancel]);
    f.assert_events(pending, &[f.down_event(pending, 0, 1), Event::Cancel]);
}

#[test]
fn cancel_retires_pending_up_before_another_clients_frame() {
    let mut f = Fixture::new();
    let previous = f.connect();
    let next = f.connect();
    f.down(previous, 0, 0);
    f.frame();
    f.assert_events(previous, &[f.down_event(previous, 0, 0), Event::Frame]);
    f.up(0);
    f.cancel();
    f.assert_events(previous, &[Event::Up(0), Event::Cancel]);

    // A different slot leaves any stale frame_pending entry observable.
    f.down(next, 0, 1);
    f.frame();
    f.assert_events(next, &[f.down_event(next, 0, 1), Event::Frame]);
    f.assert_events(previous, &[]);
}

#[test]
fn active_and_pending_up_surfaces_share_one_cancel() {
    let mut f = Fixture::new();
    let client = f.connect();
    f.down(client, 0, 0);
    f.touch.unset_grab(&mut f.server);
    f.down(client, 1, 1);
    f.frame();
    f.assert_events(
        client,
        &[
            f.down_event(client, 0, 0),
            f.down_event(client, 1, 1),
            Event::Frame,
        ],
    );
    f.up(0);
    f.cancel();
    f.frame();
    f.assert_events(client, &[Event::Up(0), Event::Cancel]);

    f.down(client, 0, 2);
    f.frame();
    f.assert_events(client, &[f.down_event(client, 0, 2), Event::Frame]);
}

#[test]
fn completed_contact_is_not_cancelled_with_a_later_sequence() {
    let mut f = Fixture::new();
    let completed = f.connect();
    let active = f.connect();
    f.down(completed, 0, 0);
    f.frame();
    f.up(0);
    f.frame();
    f.assert_events(
        completed,
        &[
            f.down_event(completed, 0, 0),
            Event::Frame,
            Event::Up(0),
            Event::Frame,
        ],
    );

    f.down(active, 0, 1);
    f.cancel();
    f.frame();
    f.assert_events(active, &[f.down_event(active, 0, 1), Event::Cancel]);
    f.assert_events(completed, &[]);
}
