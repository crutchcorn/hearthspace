use super::*;
use crate::ui::frame_layout::{self, Confirmation, Output, task::Request};

#[path = "popup_toggle_tests.rs"]
mod toggles;

struct Fixture {
    state: ShellState,
    requests: tokio::sync::mpsc::Receiver<Request>,
    snapshot: Snapshot,
}

#[test]
fn duplicate_widget_popup_anchors_to_the_activated_copy() {
    let mut fixture = Fixture::new();
    fixture.state.frame_layout.activation_origin = Some(hearthspace_ipc::frame_layout::Selection {
        output: "eDP-1".into(),
        target: hearthspace_common::frame_layout::LayoutTarget::widget("notification-copy-2"),
    });
    fixture.state.toggle_notifications("eDP-1");
    let request = fixture.requests.try_recv().unwrap();
    let anchor: serde_json::Value =
        serde_json::from_str(request.command.strip_prefix("frame-popup-anchor ").unwrap()).unwrap();
    assert_eq!(anchor["target"]["id"], "notification-copy-2");
}

#[test]
fn removed_widget_cannot_open_a_popup_after_another_copy_is_added() {
    let mut fixture = Fixture::new();
    fixture.snapshot.outputs[0]
        .widgets
        .push(hearthspace_ipc::frame_layout::Widget {
            id: "notification-copy-1".into(),
            ..Default::default()
        });
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    fixture.state.frame_layout.activation_origin = Some(hearthspace_ipc::frame_layout::Selection {
        output: "eDP-1".into(),
        target: hearthspace_common::frame_layout::LayoutTarget::widget("notification-copy-1"),
    });
    let now = Instant::now();
    let opening = fixture.notification(now);
    fixture.snapshot.outputs[0].widgets[0].id = "notification-copy-2".into();
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    opening.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
}

impl Fixture {
    fn new() -> Self {
        let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into(), "HDMI-A-1".into()]);
        let snapshot = Snapshot {
            version: 1,
            outputs: vec![
                Output {
                    output: "eDP-1".into(),
                    ..Default::default()
                },
                Output {
                    output: "HDMI-A-1".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        frame_layout::apply(&mut state, snapshot.clone());
        let (sender, requests) = tokio::sync::mpsc::channel(4);
        state.frame_layout.requests = Some(sender);
        Self {
            state,
            requests,
            snapshot,
        }
    }

    fn notification(&mut self, now: Instant) -> Opening {
        self.state.toggle_notifications("eDP-1");
        self.requests
            .try_recv()
            .unwrap()
            .open
            .unwrap()
            .acknowledge(1, now)
    }
}

#[test]
fn newer_intent_on_another_output_cancels_an_already_queued_open() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let old = fixture.notification(now);
    fixture.state.toggle_power("HDMI-A-1");
    let new = fixture
        .requests
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(2, now);
    new.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[1].show_power);
    old.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
    assert!(
        fixture.state.outputs[1].show_power,
        "stale open must not dismiss newer UI"
    );
}

#[test]
fn popup_specific_dismissal_cancels_open_before_it_becomes_visible() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    fixture.state.dismiss_notifications("eDP-1");
    opening.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
}

#[test]
fn global_dismissal_cancels_an_already_queued_open() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    fixture.state.dismiss_panels();
    opening.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
}

#[test]
fn unrelated_old_panel_dismissal_does_not_cancel_new_intent() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    fixture.state.dismiss_power("eDP-1");
    fixture.state.dismiss_notifications("HDMI-A-1");
    opening.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_notifications);
}

#[test]
fn locked_snapshot_cancels_open_even_when_cached_outputs_are_restored() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    frame_layout::apply(&mut fixture.state, Snapshot::default());
    assert!(!fixture.state.frame_layout.visible);
    assert!(!fixture.state.frame_layout.snapshot.outputs.is_empty());
    opening.clone().deliver(&mut fixture.state, now);
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    opening.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
    let fresh = fixture.notification(now);
    fresh.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_notifications);
}

#[test]
fn removed_then_readded_output_cannot_resurrect_an_opening() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    fixture.state.outputs[1].show_power = true;
    fixture.state.apply_output_names(vec!["HDMI-A-1".into()]);
    opening.clone().deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_power);
    fixture
        .state
        .apply_output_names(vec!["eDP-1".into(), "HDMI-A-1".into()]);
    opening.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
    assert!(fixture.state.outputs[1].show_power);
}

#[test]
fn editor_takeover_permanently_cancels_a_queued_open() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    let mut editing = fixture.snapshot.clone();
    editing.editing = true;
    editing.session = 9;
    frame_layout::apply(&mut fixture.state, editing);
    opening.clone().deliver(&mut fixture.state, now);
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    opening.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
}

#[test]
fn confirmation_takeover_permanently_cancels_a_queued_open() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    let mut confirming = fixture.snapshot.clone();
    confirming.confirmation = Some(Confirmation::CloseWindow { window_id: 7 });
    frame_layout::apply(&mut fixture.state, confirming);
    opening.clone().deliver(&mut fixture.state, now);
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    opening.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
}

#[test]
fn five_second_expiry_is_fixed_and_checked_again_at_ui_delivery() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    assert!(opening.valid_at(now + Duration::from_secs(4)));
    assert!(opening.valid_at(now + Duration::from_millis(4999)));
    opening.deliver(&mut fixture.state, now + Duration::from_secs(5));
    assert!(!fixture.state.outputs[0].show_notifications);
    let fresh = fixture.notification(now + Duration::from_secs(6));
    fresh.deliver(&mut fixture.state, now + Duration::from_secs(6));
    assert!(fixture.state.outputs[0].show_notifications);
}

#[test]
fn missed_takeover_epoch_cancels_an_already_queued_open_and_allows_a_fresh_intent() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let opening = fixture.notification(now);
    // The editor/lock notification was lost and the takeover ended between polls.
    // Only its persistent epoch reaches the shell, with an otherwise healthy view.
    fixture.snapshot.popup_epoch += 1;
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    opening.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
    let fresh = fixture.notification(now);
    fresh.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_notifications);
}

#[test]
fn cancellation_event_rejects_old_snapshot_but_not_fresh_intent_or_duplicate_event() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let old = fixture.notification(now);
    frame_layout::cancel_popups(&mut fixture.state, 2);
    let fresh = fixture.notification(now);
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    assert_eq!(fixture.state.frame_layout.popup_epoch, 2);
    frame_layout::cancel_popups(&mut fixture.state, 1);
    frame_layout::cancel_popups(&mut fixture.state, 2);
    fixture.snapshot.popup_epoch = 2;
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    old.deliver(&mut fixture.state, now);
    fresh.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_notifications);
}

#[test]
fn output_epoch_cancels_queued_work_without_dismissing_a_surviving_panel() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    let old = fixture.notification(now);
    fixture.state.outputs[1].show_power = true;
    frame_layout::observe_popup_epoch(&mut fixture.state, 1);
    old.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
    assert!(fixture.state.outputs[1].show_power);
}

#[test]
fn accepted_launcher_token_is_preserved_but_new_query_cancels_old_open() {
    let mut fixture = Fixture::new();
    let now = Instant::now();
    assert!(frame_layout::popups::defer(
        &mut fixture.state,
        "eDP-1",
        Popup::AppsRequest {
            token: 42,
            toggle: false
        }
    ));
    let opening = fixture
        .requests
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(1, now);
    opening.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_search);
    assert_eq!(fixture.state.outputs[0].search_request_token, Some(42));
    fixture.state.dismiss_search("eDP-1");
    assert!(frame_layout::popups::defer(
        &mut fixture.state,
        "eDP-1",
        Popup::AppsRequest {
            token: 43,
            toggle: false
        }
    ));
    let old = fixture
        .requests
        .try_recv()
        .unwrap()
        .open
        .unwrap()
        .acknowledge(2, now);
    fixture.state.change_query("eDP-1", "newer query".into());
    old.deliver(&mut fixture.state, now);
    assert_eq!(fixture.state.outputs[0].query, "newer query");
    assert_eq!(fixture.state.outputs[0].search_request_token, None);
}

#[test]
fn activating_another_copy_replaces_the_opening_instead_of_reversing_it() {
    let mut fixture = Fixture::new();
    fixture.snapshot.outputs[0].widgets = ["copy-1", "copy-2"]
        .into_iter()
        .map(|id| hearthspace_ipc::frame_layout::Widget {
            id: id.into(),
            ..Default::default()
        })
        .collect();
    frame_layout::apply(&mut fixture.state, fixture.snapshot.clone());
    let origin = |id| hearthspace_ipc::frame_layout::Selection {
        output: "eDP-1".into(),
        target: hearthspace_common::frame_layout::LayoutTarget::widget(id),
    };
    fixture.state.frame_layout.activation_origin = Some(origin("copy-1"));
    let now = Instant::now();
    let first = fixture.notification(now);
    fixture.state.frame_layout.activation_origin = Some(origin("copy-2"));
    let second = fixture.notification(now);
    first.deliver(&mut fixture.state, now);
    assert!(!fixture.state.outputs[0].show_notifications);
    second.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_notifications);
    assert_eq!(
        fixture.state.frame_layout.popup_origin,
        Some(origin("copy-2"))
    );

    fixture.state.frame_layout.activation_origin = Some(origin("copy-1"));
    let third = fixture.notification(now);
    third.deliver(&mut fixture.state, now);
    assert!(fixture.state.outputs[0].show_notifications);
    assert_eq!(
        fixture.state.frame_layout.popup_origin,
        Some(origin("copy-1"))
    );
}
