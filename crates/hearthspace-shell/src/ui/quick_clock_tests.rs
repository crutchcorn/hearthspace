//! The normal clock popup edits the same output setting as layout editing.
use super::*;
use crate::ui::{
    clock_format,
    popup::{self, PopupKind},
    quick_clock, quick_panel,
};
use hearthspace_common::frame_layout::ClockDateFormat;
use masonry::{
    accesskit::{Role, Toggled},
    core::{
        TextEvent, WidgetId, WindowEvent,
        keyboard::{Key, NamedKey},
    },
};

fn fixture() -> (ShellState, mpsc::Receiver<task::Request>) {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into(), "DP-2".into()]);
    state.frame_layout.snapshot.outputs = vec![
        Output {
            output: "eDP-1".into(),
            ..Default::default()
        },
        Output {
            output: "DP-2".into(),
            ..Default::default()
        },
    ];
    state.frame_layout.snapshot.outputs[0]
        .layout
        .clock_date_format = ClockDateFormat::Weekday;
    let (sender, receiver) = mpsc::channel(8);
    state.frame_layout.requests = Some(sender);
    state.toggle_quick_panel("eDP-1", quick_panel::Kind::Clock);
    (state, receiver)
}

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(quick_panel::panel(state, &state.outputs[0]))
}

fn radio<V: WidgetView<ShellState, Widget: Sized>>(ui: &Ui<V>, name: &str) -> WidgetId {
    ids(&ui.harness)
        .into_iter()
        .find(|&id| {
            ui.harness.access_node(id).is_some_and(|node| {
                node.role() == Role::RadioButton && node.label().as_deref() == Some(name)
            })
        })
        .unwrap_or_else(|| panic!("missing format choice {name}"))
}

fn text<V: WidgetView<ShellState, Widget: Sized>>(ui: &Ui<V>) -> Vec<String> {
    ids(&ui.harness)
        .into_iter()
        .filter_map(|id| {
            ui.harness
                .access_node(id)
                .and_then(|node| node.label().or_else(|| node.value()))
        })
        .collect()
}

fn key<V: WidgetView<ShellState, Widget: Sized>>(
    ui: &mut Ui<V>,
    key: NamedKey,
    build: &impl Fn(&ShellState) -> V,
) {
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(key)));
    ui.dispatch(build);
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(key)));
    ui.dispatch(build);
}

fn assert_keyboard_trigger_opens_choices(trigger_key: Key) {
    let (state, mut requests) = fixture();
    let mut ui = Ui::new(state, view, (320, 400));
    let trigger = ui.button("Date format");
    ui.harness
        .process_text_event(TextEvent::key_down(trigger_key.clone()));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].clock_formats.open);
    assert_eq!(ui.harness.focused_widget_id(), Some(trigger));
    assert!(requests.try_recv().is_err());

    ui.harness
        .process_text_event(TextEvent::key_up(trigger_key.clone()));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].clock_formats.open);
    assert_eq!(ui.harness.focused_widget_id(), Some(radio(&ui, "Weekday")));
    assert!(
        requests.try_recv().is_err(),
        "opening must not select a format"
    );

    key(&mut ui, NamedKey::ArrowDown, &view);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(radio(&ui, "Short date"))
    );
    ui.harness
        .process_text_event(TextEvent::key_down(trigger_key.clone()));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].clock_formats.open);
    assert!(requests.try_recv().is_err());
    ui.harness
        .process_text_event(TextEvent::key_up(trigger_key));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].clock_formats.open);
    assert!(ui.state.outputs[0].show_controls);
    assert_eq!(ui.harness.focused_widget_id(), Some(trigger));
    let payload: serde_json::Value = serde_json::from_str(
        requests
            .try_recv()
            .unwrap()
            .command
            .strip_prefix("frame-clock-format ")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        payload,
        serde_json::json!({"output":"eDP-1","value":"short"})
    );
    assert!(requests.try_recv().is_err());
}

#[test]
fn enter_opens_clock_formats_without_selecting_on_the_same_key_release() {
    assert_keyboard_trigger_opens_choices(Key::Named(NamedKey::Enter));
}

#[test]
fn space_opens_clock_formats_without_selecting_on_the_same_key_release() {
    assert_keyboard_trigger_opens_choices(Key::Character(" ".into()));
}

#[test]
fn normal_clock_dropdown_selects_format_and_restores_calendar_focus() {
    let (state, mut requests) = fixture();
    let mut ui = Ui::new(state, view, (320, 400));
    let trigger = ui.button("Date format");
    assert_eq!(ui.harness.focused_widget_id(), Some(trigger));
    assert_eq!(
        ui.harness
            .access_node(trigger)
            .unwrap()
            .data()
            .is_expanded(),
        Some(false)
    );
    ui.activate(trigger);
    ui.dispatch(&view);
    assert_eq!(
        ui.harness
            .access_node(ui.button("Date format"))
            .unwrap()
            .data()
            .is_expanded(),
        Some(true)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(radio(&ui, "Weekday")));
    for (format, name) in clock_format::FORMATS {
        let id = radio(&ui, name);
        assert_eq!(
            ui.harness.access_node(id).unwrap().toggled(),
            Some(if format == ClockDateFormat::Weekday {
                Toggled::True
            } else {
                Toggled::False
            })
        );
        assert!(
            text(&ui).contains(&clock_format::preview(format)),
            "live preview for {name}"
        );
    }
    // Arrow keys browse without mutating the saved setting; Enter commits.
    key(&mut ui, NamedKey::End, &view);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(radio(&ui, "Long date"))
    );
    assert!(requests.try_recv().is_err());
    key(&mut ui, NamedKey::Enter, &view);
    let request = requests.try_recv().unwrap();
    let payload: serde_json::Value =
        serde_json::from_str(request.command.strip_prefix("frame-clock-format ").unwrap()).unwrap();
    assert_eq!(
        payload,
        serde_json::json!({"output":"eDP-1","value":"long"})
    );
    assert!(!ui.state.outputs[0].clock_formats.open);
    assert!(ui.state.outputs[0].show_controls);
    assert_eq!(ui.harness.focused_widget_id(), Some(trigger));
    assert_eq!(
        ui.harness
            .access_node(trigger)
            .unwrap()
            .data()
            .is_expanded(),
        Some(false)
    );

    // Confirm the authoritative compositor update, as delivered by the worker.
    let mut snapshot = ui.state.frame_layout.snapshot.clone();
    snapshot.outputs[0].layout.clock_date_format = ClockDateFormat::Long;
    apply(&mut ui.state, snapshot);
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(text(&ui).contains(&format!(
        "Long date: {}",
        clock_format::preview(ClockDateFormat::Long)
    )));
    assert_eq!(ui.harness.focused_widget_id(), Some(trigger));
    assert_eq!(
        ui.state.frame_layout.snapshot.outputs[1]
            .layout
            .clock_date_format,
        ClockDateFormat::None
    );
    ui.activate(trigger);
    ui.dispatch(&view);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(radio(&ui, "Long date"))
    );
    assert_eq!(
        ui.harness
            .access_node(radio(&ui, "Long date"))
            .unwrap()
            .toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn escape_dismisses_the_entire_clock_popup_from_formats_or_calendar() {
    let (state, mut requests) = fixture();
    let mut ui = Ui::new(state, view, (320, 400));
    key(&mut ui, NamedKey::Enter, &view);
    assert!(ui.state.outputs[0].clock_formats.open);
    key(&mut ui, NamedKey::Escape, &view);
    assert!(!ui.state.outputs[0].clock_formats.open);
    assert!(!ui.state.outputs[0].show_controls);
    assert!(requests.try_recv().is_err());

    ui.state
        .toggle_quick_panel("eDP-1", quick_panel::Kind::Clock);
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].show_controls);
    assert!(!ui.state.outputs[0].clock_formats.open);
    key(&mut ui, NamedKey::Escape, &view);
    assert!(!ui.state.outputs[0].show_controls);
    assert!(requests.try_recv().is_err());
}

#[test]
fn trigger_and_left_arrow_return_to_calendar_and_tab_can_choose_a_format() {
    let (state, mut requests) = fixture();
    let mut ui = Ui::new(state, view, (320, 400));
    key(&mut ui, NamedKey::Enter, &view);
    assert!(ui.state.outputs[0].clock_formats.open);
    ui.activate(ui.button("Date format"));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].clock_formats.open);
    assert!(ui.state.outputs[0].show_controls);

    key(&mut ui, NamedKey::Enter, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(radio(&ui, "Weekday")));
    key(&mut ui, NamedKey::ArrowLeft, &view);
    assert!(!ui.state.outputs[0].clock_formats.open);
    assert!(ui.state.outputs[0].show_controls);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Date format"))
    );
    assert!(requests.try_recv().is_err());
    key(&mut ui, NamedKey::ArrowDown, &view);
    assert_eq!(ui.harness.focused_widget_id(), Some(radio(&ui, "Weekday")));
    ui.harness.press_tab_key(false);
    ui.dispatch(&view);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(radio(&ui, "Short date"))
    );
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Character(" ".into())));
    ui.dispatch(&view);
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&view);
    assert!(
        requests
            .try_recv()
            .unwrap()
            .command
            .contains("\"value\":\"short\"")
    );
    assert!(!ui.state.outputs[0].clock_formats.open);
    key(&mut ui, NamedKey::Escape, &view);
    assert!(!ui.state.outputs[0].show_controls);
}

#[test]
fn clock_formats_are_scrollable_on_a_narrow_short_output_and_reset_on_dismissal() {
    let (mut state, mut requests) = fixture();
    state.frame_layout.snapshot.outputs[0].rect = Rect {
        width: 264.0,
        height: 204.0,
        ..Default::default()
    };
    let mut ui = Ui::new(state, view, (240, 180));
    ui.activate(ui.button("Date format"));
    ui.dispatch(&view);
    let size = quick_clock::size(&ui.state, &ui.state.outputs[0]);
    assert_eq!(size.width, 240.0);
    assert_eq!(size.height, 180.0);
    ui.harness
        .process_window_event(WindowEvent::Resize((240, 180).into()));
    ui.dispatch(&view);
    key(&mut ui, NamedKey::End, &view);
    let bounds = ui
        .harness
        .access_node(radio(&ui, "Long date"))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(
        bounds.x0 >= 0.0 && bounds.x1 <= 240.0 && bounds.y0 >= 0.0 && bounds.y1 <= 180.0,
        "focused choice must be revealed: {bounds:?}"
    );
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.role() == Role::ScrollView
                && node.scroll_y_max().is_some_and(|height| height > 0.0)
        })
    }));
    ui.activate(radio(&ui, "Long date"));
    ui.dispatch(&view);
    assert!(requests.try_recv().is_ok());
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Date format"))
    );
    ui.activate(ui.button("Date format"));
    ui.dispatch(&view);
    ui.state.dismiss_panels();
    assert!(!ui.state.outputs[0].clock_formats.open);
    assert!(!ui.state.outputs[0].show_controls);
    ui.state
        .toggle_quick_panel("eDP-1", quick_panel::Kind::Clock);
    assert!(!ui.state.outputs[0].clock_formats.open);
    assert!(popup::size(&ui.state.outputs[0], PopupKind::Controls, 320.0, 200.0).height <= 400.0);
}
