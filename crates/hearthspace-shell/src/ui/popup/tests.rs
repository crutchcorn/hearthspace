use super::*;
use crate::ui::{
    launcher, notification_view, notifications::Notification, session_controls, test_support::Ui,
    window_catalog::RunningWindow,
};
use masonry::{
    core::WindowEvent,
    testing::{TestHarness, TestHarnessParams},
};
use std::sync::Arc;
use xilem::{
    ViewCtx,
    core::{RawProxy, SendMessage, View, ViewId},
    view::label,
};

fn state() -> ShellState {
    ShellState::new("/unused-popup-test".into(), vec!["eDP-1".into()])
}

fn notifications(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(notification_view::panel(
        &state.outputs[0],
        state.notifications.clone(),
        state.notifications_error.clone(),
    ))
}

fn power(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(session_controls::power_panel(state, &state.outputs[0]))
}

fn search(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(launcher::launcher_view(
        state,
        &state.outputs[0],
        launcher::results(state, &state.outputs[0]),
        None,
    ))
}

#[test]
fn search_grows_for_results_and_shrinks_when_the_query_is_cleared() {
    let mut state = state();
    state.catalog = Default::default();
    state.windows = (1..=4)
        .map(|id| RunningWindow {
            id,
            title: format!("Notes {id}"),
            app_id: "editor".into(),
            focused: false,
        })
        .collect();
    let mut ui = Ui::new(state, search, (650, 560));
    let empty = size(&ui.state.outputs[0], PopupKind::Search, 650.0, 560.0);
    assert!(
        empty.height < 560.0,
        "empty launcher should fit its command bar, layout controls and empty state: {empty:?}"
    );
    ui.button("Show app grid");
    ui.button("Draw to launch");
    ui.state.change_query("eDP-1", "Notes".into());
    ui.rebuild(&search);
    ui.dispatch(&search);
    let populated = size(&ui.state.outputs[0], PopupKind::Search, 650.0, 560.0);
    assert!(
        populated.height > empty.height,
        "four results should grow beyond the input actions"
    );
    assert!(populated.height < 560.0);
    ui.state.change_query("eDP-1", String::new());
    ui.rebuild(&search);
    ui.dispatch(&search);
    assert_eq!(
        size(&ui.state.outputs[0], PopupKind::Search, 650.0, 560.0),
        empty
    );
}

#[test]
fn empty_notifications_shrink_long_content_caps_and_removal_shrinks_again() {
    let mut ui = Ui::new(state(), notifications, (360, 440));
    let empty = size(&ui.state.outputs[0], PopupKind::Notifications, 360.0, 440.0);
    assert!(
        (250.0..300.0).contains(&empty.height),
        "empty inbox should fit its header and spacious status card: {empty:?}"
    );
    ui.harness
        .process_window_event(WindowEvent::Resize((360, empty.height as u32).into()));
    ui.dispatch(&notifications);
    assert_eq!(
        size(&ui.state.outputs[0], PopupKind::Notifications, 360.0, 440.0),
        empty
    );

    ui.state.notifications = (1..=8)
        .map(|id| Notification {
            id,
            app_name: "Notes".into(),
            summary: "A notification that wraps when needed".into(),
            body: "A long notification body with enough content to require several lines. "
                .repeat(5),
            actions: Vec::new(),
            urgency: 1,
            active: false,
        })
        .collect();
    ui.rebuild(&notifications);
    ui.dispatch(&notifications);
    assert_eq!(
        size(&ui.state.outputs[0], PopupKind::Notifications, 360.0, 440.0).height,
        440.0
    );
    assert!(
        ui.state.outputs[0].popup_measurements[PopupKind::Notifications.index()]
            .body
            .unwrap()
            > 440.0
    );

    ui.state.notifications.clear();
    ui.rebuild(&notifications);
    ui.dispatch(&notifications);
    assert_eq!(
        size(&ui.state.outputs[0], PopupKind::Notifications, 360.0, 440.0),
        empty,
        "a large previous window must not inflate the intrinsic content measurement"
    );
}

#[test]
fn power_confirmation_and_wrapped_errors_measure_their_actual_content() {
    let mut ui = Ui::new(state(), power, (300, 245));
    let measured =
        |state: &ShellState| session_controls::power_size(state, &state.outputs[0]).height;
    let normal = measured(&ui.state);
    assert!((245.0..=400.0).contains(&normal));
    ui.state.outputs[0].power_confirmation = Some(session_controls::Confirmation::LogOff);
    ui.rebuild(&power);
    ui.dispatch(&power);
    let confirm = measured(&ui.state);
    assert!(confirm < normal, "confirmation fits its smaller content");
    ui.state.outputs[0].launch_error =
        Some("The session service could not complete this request. ".repeat(20));
    ui.rebuild(&power);
    ui.dispatch(&power);
    assert_eq!(measured(&ui.state), 400.0);
    ui.harness
        .process_window_event(WindowEvent::Resize((220, 192).into()));
    ui.dispatch(&power);
    assert_eq!(measured(&ui.state), 400.0);
    ui.harness
        .process_window_event(WindowEvent::Resize((300, 245).into()));
    // Deliver the existing dialog's measurement before replacing its view.
    ui.dispatch(&power);
    ui.state.outputs[0].power_confirmation = None;
    ui.state.outputs[0].launch_error = None;
    ui.rebuild(&power);
    ui.dispatch(&power);
    assert_eq!(measured(&ui.state), normal);
}

#[derive(Debug)]
struct NoMessages;
impl RawProxy for NoMessages {
    fn send_message(
        &self,
        _: Arc<[ViewId]>,
        _: SendMessage,
    ) -> Result<(), xilem::core::ProxyError> {
        panic!("popup pixel fixture has no background tasks")
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

#[test]
fn rounded_popup_frame_has_transparent_corners_and_a_readable_glass_tint() {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let view = sized_box(frame(PopupKind::Controls, label("Popup content")));
    let (root, _) = view.build(&mut ctx, &mut state());
    let mut harness = TestHarness::create_with(
        crate::ui::appearance::properties(hearthspace_ui_components::Palette::default()),
        root.new_widget,
        TestHarnessParams::default()
            .with_size((360, 140))
            .with_background(Color::TRANSPARENT),
    );
    let image = harness.render();
    for (x, y) in [(0, 0), (359, 0), (0, 139), (359, 139)] {
        assert_eq!(
            image.get_pixel(x, y).0[3],
            0,
            "corner ({x}, {y}) must expose the compositor behind it"
        );
    }
    assert!((200..230).contains(&image.get_pixel(180, 110).0[3]));
    assert!((200..230).contains(&image.get_pixel(180, 1).0[3]));
}

#[test]
fn full_bleed_launcher_and_notification_content_preserve_rounded_corners() {
    for notification in [false, true] {
        let mut state = state();
        state.catalog = Default::default();
        state.notifications = vec![Notification {
            id: 1,
            app_name: "Notes".into(),
            summary: "Critical message".into(),
            body: "Body text ".repeat(60),
            actions: Vec::new(),
            urgency: 2,
            active: true,
        }];
        let runtime = xilem::tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
        let content = if notification {
            notifications(&state).boxed()
        } else {
            search(&state).boxed()
        };
        let view = sized_box(content);
        let (root, _) = view.build(&mut ctx, &mut state);
        let mut harness = TestHarness::create_with(
            crate::ui::appearance::properties(hearthspace_ui_components::Palette::default()),
            root.new_widget,
            TestHarnessParams::default()
                .with_size((650, 320))
                .with_background(Color::TRANSPARENT),
        );
        let image = harness.render();
        for (x, y) in [(2, 2), (647, 2), (2, 317), (647, 317)] {
            assert_eq!(
                image.get_pixel(x, y).0[3],
                0,
                "full-bleed content must not paint outside the rounded frame at ({x}, {y}); notifications={notification}"
            );
        }
        assert!(image.get_pixel(325, 5).0[3] >= 200);
        let control = if notification {
            "Close notifications"
        } else {
            "Close apps list"
        };
        let button = crate::ui::test_support::ids(&harness)
            .into_iter()
            .find(|id| {
                harness.access_node(*id).is_some_and(|node| {
                    node.role() == masonry::accesskit::Role::Button
                        && node.label().as_deref() == Some(control)
                })
            })
            .unwrap();
        harness.focus_on(Some(button));
        assert_eq!(harness.focused_widget_id(), Some(button));
    }
}
