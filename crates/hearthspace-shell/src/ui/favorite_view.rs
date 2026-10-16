//! Explicit search drawing mode; Settings owns app gesture configuration.
use super::{
    favorite_actions as actions,
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, label, sized_box},
};

pub(super) fn drawing(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> Box<AnyWidgetView<ShellState>> {
    let cancel_key = output.key().to_owned();
    let hint =
        if let Some(error) = &state.favorites.error {
            format!("Gestures unavailable: {error}")
        } else if !state.favorites.store.entries().iter().any(|entry| {
            entry.gesture.is_some() && state.catalog.app_by_id(&entry.app_id).is_some()
        }) {
            "Add an app gesture in Settings → Search → Gestures.".into()
        } else {
            "Draw an app's gesture to launch it".into()
        };
    flex_col((
        components::field_label(hint).text_size(13.0).color(components::TEXT_MUTED).line_break_mode(LineBreaking::WordWrap),
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
