use super::*;
use crate::ui::{notifications::Notification, quick_panel::Kind};
use hearthspace_common::frame_layout::ClockDateFormat;
use hearthspace_ui_components::{self as components, icons::IconWidget};
use masonry::{
    accesskit::Toggled,
    core::{
        TextEvent, WidgetId, WindowEvent,
        keyboard::{Key, NamedKey},
    },
    kurbo::Rect as Bounds,
    layout::AsUnit,
    properties::BorderColor,
    widgets::Label,
};
use xilem::style::Style;

fn status_state(id: &str, vertical: bool) -> ShellState {
    let mut state = state();
    state.frame_layout.snapshot.editing = false;
    state.frame_layout.snapshot.outputs[0].widgets = vec![Widget {
        id: id.into(),
        orientation: if vertical { "vertical" } else { "horizontal" }.into(),
        ..Default::default()
    }];
    state
}

fn surface_size(state: &ShellState) -> (u32, u32) {
    let output = &state.frame_layout.snapshot.outputs[0];
    let part = &output.widgets[0];
    let (width, height) = widgets::size(
        state,
        &part.id,
        part.orientation == "vertical",
        output.layout.is_expanded(&part.id),
        false,
        output.layout.clock_date_format,
        false,
    );
    (width.ceil() as u32, height.ceil() as u32)
}

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    let output = &state.frame_layout.snapshot.outputs[0];
    let part = &output.widgets[0];
    let chain = &state.frame_layout.chains[&widget_key("test", &part.id)];
    let content = widgets::content(
        state,
        &state.outputs[0],
        &part.id,
        part.orientation == "vertical",
        output.layout.is_expanded(&part.id),
        output.layout.clock_date_format,
        chain,
    );
    // Match parts::windows: the client surface gives the control its allocation.
    sized_box(focus::boundary(
        sized_box(content).padding(5.0.px()),
        chain.clone(),
        "test".into(),
        LayoutTarget::widget(&part.id),
        state.frame_layout.snapshot.focus_request.clone(),
    ))
}

fn bounds(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>, id: WidgetId) -> Bounds {
    let rect = ui.harness.access_node(id).unwrap().bounding_box().unwrap();
    Bounds::new(rect.x0, rect.y0, rect.x1, rect.y1)
}

fn control(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>) -> WidgetId {
    ui.button(
        if ui.state.frame_layout.snapshot.outputs[0].widgets[0].id == "clock" {
            "Date and time"
        } else {
            "Notifications"
        },
    )
}

#[track_caller]
fn inside(inner: Bounds, outer: Bounds) {
    const EPSILON: f64 = 0.001;
    assert!(
        inner.width() > 0.0
            && inner.height() > 0.0
            && inner.x0 >= outer.x0 - EPSILON
            && inner.y0 >= outer.y0 - EPSILON
            && inner.x1 <= outer.x1 + EPSILON
            && inner.y1 <= outer.y1 + EPSILON,
        "content {inner:?} must fit without clipping inside {outer:?}"
    );
}

fn direction(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>, id: WidgetId, clockwise: bool) {
    let transform = ui.harness.get_widget_with_id(id).ctx().window_transform();
    let expected = if clockwise {
        [0.0, 1.0, -1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0, 1.0]
    };
    for (actual, expected) in transform.as_coeffs()[..4].iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected clockwise={clockwise}, got {transform:?}"
        );
    }
}

fn caption(
    ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>,
    vertical: bool,
    text: Option<&str>,
) -> Bounds {
    let id = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .get_widget_with_id(*id)
                .downcast::<Label>()
                .is_some()
        })
        .expect("expanded status has a real text label");
    direction(ui, id, vertical);
    let node = ui.harness.access_node(id).unwrap();
    if let Some(text) = text {
        assert_eq!(node.value().as_deref(), Some(text));
    }
    let button = bounds(ui, control(ui));
    inside(button, bounds(ui, ui.harness.root_id()));
    let label = bounds(ui, id);
    inside(label, button);
    let widget = ui.harness.get_widget_with_id(id);
    let painted_label = widget
        .ctx()
        .window_transform()
        .transform_rect_bbox(widget.ctx().border_box());
    inside(painted_label, button);
    let runs: Vec<_> = node
        .children()
        .filter(|node| node.role() == Role::TextRun)
        .map(|node| {
            let rect = node.bounding_box().unwrap();
            Bounds::new(rect.x0, rect.y0, rect.x1, rect.y1)
        })
        .collect();
    assert!(!runs.is_empty(), "the caption must contain laid-out text");
    for run in &runs {
        inside(*run, label);
        inside(*run, painted_label);
        inside(*run, button);
    }
    runs.into_iter().reduce(|a, b| a.union(b)).unwrap()
}

fn icon(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>) -> Bounds {
    let id = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .get_widget_with_id(*id)
                .downcast::<IconWidget>()
                .is_some()
        })
        .expect("the status icon stays present");
    direction(ui, id, false);
    let rect = bounds(ui, id);
    assert_eq!((rect.width(), rect.height()), (18.0, 18.0));
    inside(rect, bounds(ui, control(ui)));
    rect
}

#[test]
fn vertical_all_clear_rotates_only_the_caption_and_keeps_the_bell_above_it() {
    let state = status_state("notifications", false);
    let size = surface_size(&state);
    let horizontal = Ui::new(state, view, size);
    let before = bounds(&horizontal, control(&horizontal));
    assert_eq!(before.height(), 44.0);
    let text = caption(&horizontal, false, Some("All clear"));
    let bell = icon(&horizontal);
    assert!(
        bell.x1 < text.x0,
        "the horizontal bell precedes its caption"
    );

    let state = status_state("notifications", true);
    let size = surface_size(&state);
    let vertical = Ui::new(state, view, size);
    let after = bounds(&vertical, control(&vertical));
    assert_eq!(after.width(), 44.0);
    assert!((after.height() - before.width()).abs() <= 1.0);
    let text = caption(&vertical, true, Some("All clear"));
    let bell = icon(&vertical);
    assert!(
        bell.y1 < text.y0,
        "the upright bell must be above its caption"
    );
    assert!((bell.center().x - text.center().x).abs() <= 1.0);
}

#[test]
fn every_clock_date_format_fits_as_one_clockwise_line_in_a_44_pixel_column() {
    for format in [
        ClockDateFormat::None,
        ClockDateFormat::Weekday,
        ClockDateFormat::Short,
        ClockDateFormat::Numeric,
        ClockDateFormat::Long,
    ] {
        let mut state = status_state("clock", false);
        state.frame_layout.snapshot.outputs[0]
            .layout
            .clock_date_format = format;
        let size = surface_size(&state);
        let horizontal = Ui::new(state, view, size);
        let before = bounds(&horizontal, control(&horizontal));
        let horizontal_text = caption(&horizontal, false, None);
        assert_eq!(before.height(), 44.0, "{format:?}");

        let mut state = status_state("clock", true);
        state.frame_layout.snapshot.outputs[0]
            .layout
            .clock_date_format = format;
        let size = surface_size(&state);
        let vertical = Ui::new(state, view, size);
        let after = bounds(&vertical, control(&vertical));
        let vertical_text = caption(&vertical, true, None);
        assert_eq!(after.width(), 44.0, "{format:?}");
        assert!((after.height() - before.width()).abs() <= 1.0, "{format:?}");
        assert!(
            (vertical_text.height() - horizontal_text.width()).abs() <= 1.0,
            "{format:?}"
        );
        assert!(vertical_text.height() > vertical_text.width(), "{format:?}");
        assert!(
            !ids(&vertical.harness).into_iter().any(|id| {
                vertical
                    .harness
                    .get_widget_with_id(id)
                    .downcast::<IconWidget>()
                    .is_some()
            }),
            "an expanded clock is text only"
        );
    }
}

#[test]
fn icon_only_clock_and_notifications_stay_upright_44_pixel_squares() {
    for id in ["clock", "notifications"] {
        for vertical in [false, true] {
            let mut state = status_state(id, vertical);
            state.frame_layout.snapshot.outputs[0]
                .layout
                .expanded
                .insert(id.into(), false);
            let size = surface_size(&state);
            let ui = Ui::new(state, view, size);
            let button = bounds(&ui, control(&ui));
            assert_eq!((button.width(), button.height()), (44.0, 44.0));
            inside(button, bounds(&ui, ui.harness.root_id()));
            icon(&ui);
            assert!(
                !ids(&ui.harness).into_iter().any(|id| {
                    ui.harness
                        .get_widget_with_id(id)
                        .downcast::<Label>()
                        .is_some()
                }),
                "icon-only {id} must not leave a hidden caption taking space"
            );
        }
    }
}

#[test]
fn orientation_and_unread_updates_keep_the_same_focused_control_and_visible_text() {
    for id in ["clock", "notifications"] {
        let mut state = status_state(id, false);
        state.frame_layout.snapshot.outputs[0]
            .layout
            .clock_date_format = ClockDateFormat::Long;
        let size = surface_size(&state);
        let mut ui = Ui::new(state, view, size);
        let button = control(&ui);
        ui.harness.press_tab_key(false);
        ui.dispatch(&view);
        for (vertical, count, text) in [
            (true, 1, "1 new"),
            (true, 24, "24 new"),
            (false, 24, "24 new"),
            (true, 0, "All clear"),
            (false, 0, "All clear"),
        ] {
            ui.state.frame_layout.snapshot.outputs[0].widgets[0].orientation =
                if vertical { "vertical" } else { "horizontal" }.into();
            ui.state.frame_layout.snapshot.outputs[0]
                .layout
                .clock_date_format = if count == 1 {
                ClockDateFormat::None
            } else {
                ClockDateFormat::Long
            };
            ui.state.notifications = (1..=count)
                .map(|id| Notification {
                    id,
                    app_name: "Fixture".into(),
                    summary: "New notification".into(),
                    body: String::new(),
                    actions: vec![],
                    urgency: 1,
                    active: true,
                })
                .collect();
            ui.rebuild(&view);
            ui.harness
                .process_window_event(WindowEvent::Resize(surface_size(&ui.state).into()));
            ui.dispatch(&view);
            assert_eq!(control(&ui), button);
            assert_eq!(ui.harness.focused_widget_id(), Some(button));
            let rect = bounds(&ui, button);
            assert_eq!(
                if vertical {
                    rect.width()
                } else {
                    rect.height()
                },
                44.0
            );
            let text = caption(&ui, vertical, (id == "notifications").then_some(text));
            if id == "notifications" {
                let bell = icon(&ui);
                assert!(if vertical {
                    bell.y1 < text.y0
                } else {
                    bell.x1 < text.x0
                });
            }
        }
    }
}

#[test]
fn vertical_status_controls_open_and_close_their_popups_from_the_keyboard() {
    for id in ["clock", "notifications"] {
        let state = status_state(id, true);
        let size = surface_size(&state);
        let mut ui = Ui::new(state, view, size);
        let button = control(&ui);
        ui.harness.press_tab_key(false);
        ui.dispatch(&view);
        assert_eq!(ui.harness.focused_widget_id(), Some(button));
        let before = bounds(&ui, button);
        for (key, open) in [
            (Key::Named(NamedKey::Enter), true),
            (Key::Character(" ".into()), false),
        ] {
            ui.harness
                .process_text_event(TextEvent::key_down(key.clone()));
            ui.harness.process_text_event(TextEvent::key_up(key));
            ui.dispatch(&view);
            if id == "clock" {
                assert_eq!(ui.state.outputs[0].show_controls, open);
                assert_eq!(ui.state.outputs[0].quick_panel, Kind::Clock);
                assert!(!ui.state.outputs[0].show_notifications);
            } else {
                assert_eq!(ui.state.outputs[0].show_notifications, open);
                assert!(!ui.state.outputs[0].show_controls);
            }
            assert_eq!(
                ui.harness.access_node(button).unwrap().toggled(),
                Some(if open { Toggled::True } else { Toggled::False })
            );
            assert_eq!(ui.harness.focused_widget_id(), Some(button));
            assert_eq!(bounds(&ui, button), before);
            assert_eq!(
                ui.harness
                    .get_widget_with_id(button)
                    .get_prop::<BorderColor>()
                    .color,
                components::FOCUS_RING_ON_GLASS
            );
            caption(&ui, true, (id == "notifications").then_some("All clear"));
        }
    }
}
