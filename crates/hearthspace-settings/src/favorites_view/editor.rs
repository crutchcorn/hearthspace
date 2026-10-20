use super::{controls, passive::passive, picker};
use crate::state::SettingsApp;
use hearthspace_shell::gesture_pad::GesturePad;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::{AsUnit, Dim, UnitPoint},
        properties::{Dimensions, LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, ZStackExt, flex_col, flex_row, label, sized_box, zstack},
};

pub(super) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let favorites = &state.favorites;
    let record = favorites.recording.as_ref().unwrap();
    let name = favorites.name(&record.app_id);
    let prompt = record.points.is_empty().then(|| {
        flex_col((
            icon(Icon::Pencil, 30.0).color(components::ACCENT_STRONG),
            label("Record a gesture").text_size(20.0),
            label(format!("One continuous stroke for {name}."))
                .text_size(10.0)
                .color(components::TEXT_MUTED)
                .line_break_mode(LineBreaking::WordWrap),
        ))
        .gap(12.0.px())
    });
    let pad = GesturePad {
        output_key: record.app_id.clone(),
        context: record.generation,
        capture_locally: true,
        status: if record.valid {
            "Shape ready to save"
        } else {
            "Draw a shape"
        }
        .into(),
        preview: record.points.clone(),
        on_stroke: |state: &mut SettingsApp, id, generation, points| {
            state.favorites.stroke(id, generation, points)
        },
        on_bounds: |_, _, _, _| {},
    };
    let drawing = zstack((
        sized_box(pad).dims(Dimensions::STRETCH),
        // Stable children keep pad identity when Retry reinserts the prompt.
        passive(sized_box(flex_col((prompt,))).padding(22.0.px())),
        passive(
            sized_box(
                label("Draw here")
                    .text_size(9.0)
                    .color(components::TEXT_MUTED),
            )
            .padding(Padding::from_vh(14.0.px(), 14.0.px())),
        )
        .alignment(UnitPoint::TOP_LEFT),
        sized_box(controls::text(Icon::RotateCcw, "Retry", false, |state| {
            state.favorites.retry()
        }))
        .width(Dim::MaxContent)
        .padding(8.0.px())
        .alignment(UnitPoint::BOTTOM_RIGHT),
    ))
    .height(
        if favorites.view_width <= 480.0 {
            205.0
        } else {
            236.0
        }
        .px(),
    );
    flex_col((
        flex_row((
            controls::icon_button(Icon::ArrowLeft, "Back to gestures", false, |state| {
                state.favorites.cancel()
            }),
            label(if favorites.editing {
                "Edit gesture"
            } else {
                "Add gesture"
            })
            .text_size(26.0),
        ))
        .gap(12.0.px()),
        flex_col((
            picker::view(state),
            drawing,
            controls::save(!record.valid || favorites.store.is_none()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(12.0.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(24.0.px())
    .boxed()
}
