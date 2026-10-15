//! Search shows favorites and explicit drawing; Settings owns configuration.
use super::{
    favorite_actions as actions,
    favorite_state::Mode,
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_row, label, sized_box},
};

pub(super) fn browse(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> Vec<Box<AnyWidgetView<ShellState>>> {
    let mut rows = Vec::new();
    if let Some(error) = &state.favorites.error {
        rows.push(
            label(format!("Favorites unavailable: {error}"))
                .text_size(13.0)
                .color(components::TEXT_MUTED)
                .line_break_mode(LineBreaking::WordWrap)
                .boxed(),
        );
    }
    let draw_key = output.key().to_owned();
    let manage_key = draw_key.clone();
    let enabled = state
        .favorites
        .store
        .entries()
        .iter()
        .any(|f| f.gesture.is_some() && state.catalog.app_by_id(&f.app_id).is_some());
    rows.push(
        flex_col((
            components::section_title("Favorites"),
            flex_row((
                button(label("Draw to launch"), move |state: &mut ShellState| {
                    actions::start(state, &draw_key, Mode::Launch)
                })
                .disabled(!enabled),
                button(label("Manage favorites"), move |state: &mut ShellState| {
                    actions::manage(state, &manage_key)
                }),
            ))
            .gap(8.0.px()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(6.0.px())
        .boxed(),
    );
    if state.favorites.store.entries().is_empty() {
        rows.push(
            label("Add apps and assign gestures in Settings → Favorites.")
                .text_size(13.0)
                .color(components::TEXT_MUTED)
                .line_break_mode(LineBreaking::WordWrap)
                .boxed(),
        );
    }
    rows
}

pub(super) fn missing(state: &ShellState) -> Vec<Box<AnyWidgetView<ShellState>>> {
    state
        .favorites
        .store
        .entries()
        .iter()
        .filter(|f| state.catalog.app_by_id(&f.app_id).is_none())
        .map(|f| {
            label(format!(
                "{} — no longer installed. Update favorites in Settings.",
                f.app_id
            ))
            .text_size(13.0)
            .color(components::TEXT_MUTED)
            .line_break_mode(LineBreaking::WordWrap)
            .boxed()
        })
        .collect()
}

pub(super) fn drawing(output: &ShellOutputInstance) -> Box<AnyWidgetView<ShellState>> {
    let cancel_key = output.key().to_owned();
    flex_col((
        components::field_label("Draw a favorite's gesture to launch it").text_size(13.0).color(components::TEXT_MUTED).line_break_mode(LineBreaking::WordWrap),
        label("Click and drag with a mouse or trackpad, or draw with one finger on the touchscreen. Use one continuous stroke inside the area.").text_size(13.0).color(components::TEXT_MUTED).line_break_mode(LineBreaking::WordWrap),
        sized_box(super::gesture_pad::GesturePad {
            output_key: output.key().to_owned(), context: output.drawing.generation,
            status: output.drawing.message.clone().unwrap_or_else(|| "Ready to draw".into()),
            preview: output.drawing.preview.clone(), capture_locally: false,
            on_stroke: super::favorite_events::local_stroke,
            on_bounds: super::favorite_events::bounds,
        }).height(220.0.px()),
        sized_box(label(output.drawing.message.clone().unwrap_or_else(|| "Lift your finger or release the button to launch a clear match.".into())).text_size(13.0).color(components::TEXT_MUTED).line_break_mode(LineBreaking::WordWrap)).height(52.0.px()),
        button(label("Back to search"), move |state: &mut ShellState| actions::cancel(state, &cancel_key)),
    )).cross_axis_alignment(CrossAxisAlignment::Stretch).gap(8.0.px()).boxed()
}
