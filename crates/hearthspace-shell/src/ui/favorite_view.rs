//! The launcher drawing tab reuses the compositor-guarded gesture recognizer.
use super::{
    favorite_actions as actions,
    favorite_state::Mode,
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        properties::{Dimensions, LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, FlexExt, FlexSpacer, flex_col, flex_row, label, sized_box, zstack},
};

pub(super) fn drawing(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> Box<AnyWidgetView<ShellState>> {
    let settings_key = output.key().to_owned();
    let retry_key = settings_key.clone();
    let prompt = output.drawing.preview.is_empty().then(|| {
        flex_col((
            icon(Icon::Pencil, 30.0).color(components::ACCENT_STRONG),
            label("Draw a gesture")
                .text_size(20.0)
                .color(components::TEXT),
            label("Release to open.")
                .text_size(10.0)
                .color(components::TEXT_MUTED),
        ))
        .gap(12.0.px())
    });
    let pad = super::gesture_pad::GesturePad {
        output_key: output.key().to_owned(),
        context: output.drawing.generation,
        status: output
            .drawing
            .message
            .clone()
            .unwrap_or_else(|| "Ready to draw".into()),
        preview: output.drawing.preview.clone(),
        capture_locally: false,
        on_stroke: super::favorite_events::local_stroke,
        on_bounds: super::favorite_events::bounds,
    };
    let card = sized_box(
        flex_col((
            flex_row((
                label("Draw here")
                    .text_size(9.0)
                    .color(components::TEXT_MUTED),
                FlexSpacer::Flex(1.0),
                super::launcher::controls::icon_control(
                    Icon::Settings,
                    "Settings",
                    None,
                    move |state| actions::manage(state, &settings_key),
                ),
            ))
            .height(44.0.px()),
            zstack((
                // Keep both stack children stable. Xilem's ZStack splice
                // appends reinserted optional children, losing the pad's index.
                flex_col((prompt,)),
                sized_box(pad).dims(Dimensions::STRETCH),
            ))
            .height(
                if output.launcher_width <= 480.0 {
                    145.0
                } else {
                    176.0
                }
                .px(),
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
    .padding(Padding::from_vh(8.0.px(), 14.0.px()))
    .background_color(components::PAPER)
    .border(components::CHROME_BORDER, 1.0.px())
    .corner_radius(14.0.px());
    let retry = output.drawing.message.is_some().then(|| {
        super::search_result::control(
            "Clear drawing and retry",
            None,
            flex_row((icon(Icon::RotateCcw, 13.0), label("Retry").text_size(11.0))).gap(7.0.px()),
            move |state| actions::start(state, &retry_key, Mode::Launch),
        )
        .height(44.0.px())
        .padding(10.0.px())
        .border(components::CHROME_BORDER, 1.0.px())
        .corner_radius(10.0.px())
    });
    // Completing a stroke must not resize its popup and invalidate the
    // compositor-authorized drawing bounds. Longer errors may still wrap.
    let feedback = flex_row((
        sized_box(label("")).dims(Dimensions::fixed(0.0.px(), 44.0.px())),
        flex_row((
            label(output.drawing.message.clone().unwrap_or_default())
                .text_size(11.0)
                .color(components::TEXT_MUTED)
                .line_break_mode(LineBreaking::WordWrap)
                .flex(1.0),
            retry,
        ))
        .gap(10.0.px())
        .flex(1.0),
    ))
    .gap(0.0.px());
    let mut rows = vec![card.boxed(), feedback.boxed()];
    let hint =
        if let Some(error) = &state.favorites.error {
            Some(format!("Gestures unavailable: {error}"))
        } else if !state.favorites.store.entries().iter().any(|entry| {
            entry.gesture.is_some() && state.catalog.app_by_id(&entry.app_id).is_some()
        }) {
            Some("Add an app gesture in Settings → Gestures.".into())
        } else {
            None
        };
    if let Some(hint) = hint {
        rows.push(
            label(hint)
                .text_size(11.0)
                .color(components::TEXT_MUTED)
                .line_break_mode(LineBreaking::WordWrap)
                .boxed(),
        );
    }
    flex_col(rows)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(12.0.px())
        .boxed()
}
