//! Clock preferences live in the clock's existing, output-anchored popup.
use super::{
    ShellState, clock_format,
    popup::{self, PopupKind},
    search_focus::{FocusTarget, Record, focus_on_activation},
    state::ShellOutputInstance,
    tray_button::{ButtonView, Target},
    tray_widget::{Appearance, ButtonAction},
};
use hearthspace_common::frame_layout::ClockDateFormat;
use xilem::{
    WidgetView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, flex_col, label},
};

#[derive(Debug, Default)]
pub(super) struct State {
    pub open: bool,
    trigger: FocusTarget,
    choices: [FocusTarget; 5],
}

fn selected(state: &ShellState, output: &str) -> ClockDateFormat {
    state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|item| item.output == output)
        .map_or(ClockDateFormat::None, |item| item.layout.clock_date_format)
}

pub(super) fn size(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> xilem::dpi::LogicalSize<f64> {
    let mut size = popup::size(output, PopupKind::Controls, 320.0, 200.0);
    if let Some(frame) = state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|frame| frame.output == output.key())
    {
        if frame.rect.width > 0.0 {
            size.width = size.width.min((frame.rect.width - 24.0).max(1.0));
        }
        if frame.rect.height > 0.0 {
            size.height = size.height.min((frame.rect.height - 24.0).max(1.0));
        }
    }
    size
}

fn set_open(state: &mut ShellState, key: &str, open: bool) {
    if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
        output.clock_formats.open = open;
        PopupKind::Controls.reset(output);
    }
}

fn back(state: &mut ShellState, key: &str) {
    set_open(state, key, false);
}

pub(super) fn button_event(state: &mut ShellState, key: &str, action: ButtonAction) {
    match action {
        ButtonAction::Activate => {
            let open = state
                .outputs
                .iter()
                .any(|output| output.key() == key && output.clock_formats.open);
            set_open(state, key, !open);
        }
        ButtonAction::Step(_) => set_open(state, key, true),
        ButtonAction::Dismiss => state.dismiss_controls(key),
        ButtonAction::Back => back(state, key),
        _ => {}
    }
}

fn choose(state: &mut ShellState, key: &str, format: ClockDateFormat) {
    // The compositor owns persistence and publishes the confirmed setting in
    // the next snapshot, keeping both this popup and its frame label in sync.
    state.frame_layout.error = None;
    super::frame_layout::submit(
        state,
        format!(
            "frame-clock-format {}",
            serde_json::json!({"output":key,"value":format})
        ),
    );
    set_open(state, key, false);
}

pub(super) fn panel(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> impl WidgetView<ShellState> + use<> {
    let selected = selected(state, output.key());
    let open = output.clock_formats.open;
    let palette = state.appearance.palette;
    let key = output.key().to_owned();
    let trigger = Record {
        inner: ButtonView {
            output: key.clone(),
            target: Target::ClockFormats,
            appearance: Appearance {
                name: "Date format".into(),
                description: clock_format::FORMATS[clock_format::index(selected)]
                    .1
                    .into(),
                expanded: Some(open),
                submenu: true,
                activate_on_key_release: true,
                ..Default::default()
            },
        },
        target: output.clock_formats.trigger.clone(),
    };
    let options = open.then(|| {
        let key = key.clone();
        clock_format::choices(
            selected,
            palette,
            Some(&output.clock_formats.choices),
            move |state, format| {
                choose(state, &key, format);
            },
        )
    });
    let summary = (!open).then(|| {
        label(format!(
            "{}: {}",
            clock_format::FORMATS[clock_format::index(selected)].1,
            clock_format::preview(selected)
        ))
        .font("Liberation Sans")
        .text_size(12.0)
        .color(palette.muted)
        .line_break_mode(LineBreaking::WordWrap)
    });
    let content = flex_col((
        (!open).then(super::quick_panel::calendar::view),
        trigger,
        summary,
        options,
        state.frame_layout.error.clone().map(|error| {
            label(error)
                .text_size(12.0)
                .color(palette.muted)
                .line_break_mode(LineBreaking::WordWrap)
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(8.0.px());
    let target = if open {
        output.clock_formats.choices[clock_format::index(selected)].clone()
    } else {
        output.clock_formats.trigger.clone()
    };
    focus_on_activation(
        popup::panel_named(output, PopupKind::Controls, "Date and time", content),
        target,
        key,
    )
    .with_dismiss(ShellState::dismiss_controls)
    .with_back(if open { Some(back) } else { None })
    .dismiss_directly()
    .with_arrow_navigation(if open {
        output.clock_formats.choices.to_vec()
    } else {
        Vec::new()
    })
}
