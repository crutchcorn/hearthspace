use crate::state::SettingsApp;
use hearthspace_shell::gesture_pad::{GesturePad, thumbnail};
use hearthspace_ui_components as components;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, properties::LineBreaking},
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_row, label, sized_box},
};

fn note(text: &'static str) -> impl WidgetView<SettingsApp> + use<> {
    label(text)
        .text_size(13.0)
        .color(components::TEXT_MUTED)
        .line_break_mode(LineBreaking::WordWrap)
}

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let content = if state.favorites.recording.is_some() {
        recording(state)
    } else {
        browsing(state)
    };
    flex_col((
        components::section_title("Gestures"),
        note("Assign a shape to an app, then draw it in search to launch the app immediately."),
        state.favorites.message.as_ref().map(|message| {
            components::field_label(message.clone()).line_break_mode(LineBreaking::WordWrap)
        }),
        content,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(12.0.px())
    .boxed()
}

fn browsing(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let favorites = &state.favorites;
    let saved: Vec<_> = favorites
        .assigned()
        .map(|favorite| {
            let id = favorite.app_id.clone();
            let name = favorites.name(&id);
            let installed = favorites.catalog.app_by_id(&id).is_some();
            let assign_id = id.clone();
            let remove_id = id.clone();
            let shape = favorite
                .gesture
                .as_ref()
                .map(|gesture| thumbnail::<SettingsApp>(gesture.points().to_vec()));
            let assign_label = format!("Change gesture for {name}");
            components::settings_section(
                name.clone(),
                flex_col((
                    (!installed).then(|| note("No longer installed. You can remove this gesture.")),
                    flex_row((
                        shape,
                        flex_col((
                            button(label(assign_label), move |state: &mut SettingsApp| {
                                state.favorites.start(&assign_id)
                            })
                            .disabled(!installed),
                            button(
                                label(format!("Remove gesture for {name}")),
                                move |state: &mut SettingsApp| {
                                    state.favorites.remove_gesture(&remove_id)
                                },
                            ),
                        ))
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .gap(4.0.px()),
                    ))
                    .gap(12.0.px()),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(6.0.px()),
            )
            .boxed()
        })
        .collect();
    let matches = favorites.catalog.search(&favorites.query, 24);
    let apps: Vec<_> = matches
        .into_iter()
        .filter(|app| {
            !favorites
                .assigned()
                .any(|favorite| favorite.app_id == app.id)
        })
        .map(|app| {
            let id = app.id;
            button(
                label(format!("Add gesture for {}", app.name)),
                move |state: &mut SettingsApp| state.favorites.start(&id),
            )
            .disabled(favorites.store.is_none())
            .boxed()
        })
        .collect();
    let empty = apps.is_empty();
    flex_col((
        button(
            label("Reload gestures and apps"),
            |state: &mut SettingsApp| state.favorites.reload(),
        ),
        saved,
        components::section_title("Add gestures"),
        crate::favorites_search::input(&favorites.query),
        empty.then(|| note("No more matching apps. Try a different search.")),
        flex_col(apps)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(4.0.px()),
        note("Showing up to 24 matching apps. Type a name to find more."),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(10.0.px())
    .boxed()
}

fn recording(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let favorites = &state.favorites;
    let record = favorites.recording.as_ref().unwrap();
    let name = favorites.name(&record.app_id);
    flex_col((
        components::section_title(format!("Gesture for {name}")),
        note("Click and drag with a mouse or trackpad, or draw with one finger on the touchscreen. Keep the whole stroke inside the area. A rough shape is enough; draw similar shapes in a consistent orientation."),
        sized_box(GesturePad {
            output_key: record.app_id.clone(), context: record.generation, capture_locally: true,
            status: if record.valid { "Shape ready to save" } else { "Draw a shape" }.into(),
            preview: record.points.clone(),
            on_stroke: |state: &mut SettingsApp, id, generation, points| state.favorites.stroke(id, generation, points),
            on_bounds: |_, _, _, _| {},
        }).fixed_height(220.0.px()),
        flex_row((
            button(label("Save gesture"), |state: &mut SettingsApp| state.favorites.save()).disabled(!record.valid || favorites.store.is_none()),
            button(label("Try again"), |state: &mut SettingsApp| state.favorites.retry()),
            button(label("Cancel recording"), |state: &mut SettingsApp| state.favorites.cancel()),
        )).gap(8.0.px()),
    )).cross_axis_alignment(CrossAxisAlignment::Stretch).gap(10.0.px()).boxed()
}

#[cfg(test)]
mod tests;
