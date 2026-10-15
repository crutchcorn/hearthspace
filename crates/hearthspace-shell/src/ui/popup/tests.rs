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
    sized_box(session_controls::power_panel(
        &state.outputs[0],
        &state.power,
        state.battery.as_ref(),
    ))
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
    let mut ui = Ui::new(state, search, (768, 560));
    let empty = size(&ui.state.outputs[0], PopupKind::Search, 768.0, 560.0);
    assert!(
        empty.height < 180.0,
        "empty search should fit its hint: {empty:?}"
    );
    ui.state.change_query("eDP-1", "Notes".into());
    ui.rebuild(&search);
    ui.dispatch(&search);
    let populated = size(&ui.state.outputs[0], PopupKind::Search, 768.0, 560.0);
    assert!(populated.height >= empty.height + 100.0);
    assert!(populated.height < 360.0);
    ui.state.change_query("eDP-1", String::new());
    ui.rebuild(&search);
    ui.dispatch(&search);
    assert_eq!(
        size(&ui.state.outputs[0], PopupKind::Search, 768.0, 560.0),
        empty
    );
}

#[test]
fn empty_notifications_shrink_long_content_caps_and_removal_shrinks_again() {
    let mut ui = Ui::new(state(), notifications, (360, 440));
    let empty = size(&ui.state.outputs[0], PopupKind::Notifications, 360.0, 440.0);
    assert!(
        empty.height < 140.0,
        "empty inbox should fit its title and one line: {empty:?}"
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
    let mut ui = Ui::new(state(), power, (300, 230));
    let normal = size(&ui.state.outputs[0], PopupKind::Power, 300.0, 230.0).height;
    assert!(
        normal < 260.0,
        "power actions should fit their content: {normal}"
    );
    ui.state.outputs[0].confirm_logout = true;
    ui.rebuild(&power);
    ui.dispatch(&power);
    let confirm = size(&ui.state.outputs[0], PopupKind::Power, 300.0, 230.0).height;
    assert!(
        confirm < normal,
        "confirmation should shrink after hiding suspend and its unavailable hint"
    );
    ui.state.outputs[0].launch_error =
        Some("The session service could not complete this request. ".repeat(20));
    ui.rebuild(&power);
    ui.dispatch(&power);
    assert_eq!(
        size(&ui.state.outputs[0], PopupKind::Power, 300.0, 230.0).height,
        400.0
    );
    let wide = ui.state.outputs[0].popup_measurements[PopupKind::Power.index()]
        .body
        .unwrap();
    ui.harness
        .process_window_event(WindowEvent::Resize((220, 192).into()));
    ui.dispatch(&power);
    let narrow = ui.state.outputs[0].popup_measurements[PopupKind::Power.index()]
        .body
        .unwrap();
    assert!(
        narrow > wide,
        "actual constrained width must remeasure wrapped errors"
    );
    ui.state.outputs[0].confirm_logout = false;
    ui.state.outputs[0].launch_error = None;
    ui.harness
        .process_window_event(WindowEvent::Resize((300, 230).into()));
    ui.rebuild(&power);
    ui.dispatch(&power);
    assert_eq!(
        size(&ui.state.outputs[0], PopupKind::Power, 300.0, 230.0).height,
        normal
    );
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
fn rounded_popup_frame_has_transparent_corners_and_opaque_content() {
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let view = sized_box(frame(label("Popup content")));
    let (root, _) = view.build(&mut ctx, &mut state());
    let mut harness = TestHarness::create_with(
        components::hearthspace_light_properties(),
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
    assert_eq!(image.get_pixel(180, 110).0, [255, 255, 255, 255]);
    assert!(image.get_pixel(180, 1).0[3] > 240);
}
