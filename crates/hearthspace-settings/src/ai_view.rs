use hearthspace_ai::DevicePreference;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{layout::AsUnit, parley::style::FontWeight, properties::Padding},
    style::Style,
    view::{
        CrossAxisAlignment, button, flex_col, flex_item, flex_row, label, progress_bar, sized_box,
    },
};

use crate::{
    ai::Operation,
    page_components::{card, disclosure, note, section},
    state::SettingsApp,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let ai = &state.ai;
    let busy = ai.pending.is_some();
    let installed = ai.installed();
    flex_col((
        flex_col((
            card(
                flex_row((
                    flex_item(label("Enable AI features").text_size(11.0), 1.0),
                    action("Download D1 model", Operation::Install, busy || installed),
                ))
                .gap(12.0.px()),
            ),
            (!installed).then(|| note("Download the optional model to get started.")),
            note(ai.message.clone()),
            busy.then(|| progress_bar(ai.progress)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(10.0.px()),
        section(
            "Features",
            flex_col((
                feature(
                    "On-device AI",
                    "AI runs locally on your device. Window titles are processed on this device. Installing the model requires a download.",
                    0,
                    ai.features_open[0],
                ),
                feature(
                    "Gather similar windows",
                    "Select one or more windows, then choose Gather similar windows. AI finds related windows on your canvas and brings them into view beside your selection.",
                    1,
                    ai.features_open[1],
                ),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ),
        section(
            "Local model",
            flex_col((
                label("LiquidAI D1").text_size(11.0).weight(FontWeight::MEDIUM),
                note(match &ai.snapshot {
                    None => "Model status unknown",
                    Some(status) if status.installed => "Model installed",
                    Some(status) if status.downloaded => "Model downloaded; setup incomplete",
                    Some(_) => "Model not installed",
                }),
                flex_row((
                    action("Refresh status", Operation::Refresh, busy),
                    installed.then(|| {
                        action(
                            "Load D1 model",
                            Operation::Warm,
                            busy || ai.snapshot.as_ref().is_some_and(|status| status.warm),
                        )
                    }),
                    action("Delete D1 model", Operation::Remove, busy || !ai.can_remove()),
                ))
                .gap(8.0.px()),
                note("The model stays ready during your session. Deleting it releases its memory and removes the download."),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ),
        processing(state),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(22.0.px())
    .boxed()
}

fn action(
    name: &'static str,
    operation: Operation,
    disabled: bool,
) -> impl WidgetView<SettingsApp> {
    components::button_surface(
        button(
            label(name).text_size(11.0),
            move |state: &mut SettingsApp| {
                state.ai.request(operation);
            },
        )
        .disabled(disabled),
        components::ButtonSurface::Control,
        false,
    )
    .height(34.0.px())
    .padding(Padding::from_vh(7.0.px(), 10.0.px()))
}

fn feature(
    name: &'static str,
    description: &'static str,
    index: usize,
    open: bool,
) -> impl WidgetView<SettingsApp> {
    sized_box(
        flex_col((
            disclosure(
                name,
                open,
                false,
                flex_row((
                    flex_item(label(name).text_size(11.0).weight(FontWeight::MEDIUM), 1.0),
                    icon(
                        if open {
                            Icon::ChevronUp
                        } else {
                            Icon::ChevronDown
                        },
                        14.0,
                    )
                    .color(components::TEXT_MUTED),
                ))
                .gap(12.0.px()),
                move |state: &mut SettingsApp| {
                    state.ai.features_open[index] = !state.ai.features_open[index];
                },
            )
            .height(44.0.px())
            .padding(12.0.px()),
            open.then(|| {
                sized_box(note(description)).padding(Padding {
                    top: 0.0.px(),
                    bottom: 12.0.px(),
                    left: 12.0.px(),
                    right: 12.0.px(),
                })
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(0.0.px()),
    )
    .background_color(components::SURFACE_SUBTLE)
    .border(components::BORDER, 1.0.px())
    .corner_radius(components::RADIUS_TILE.px())
}

fn processing(state: &SettingsApp) -> impl WidgetView<SettingsApp> {
    let busy = state.ai.pending.is_some();
    let device = state.ai.device();
    let option = move |name, preference| {
        components::button_surface(
            button(
                label(name).text_size(11.0),
                move |state: &mut SettingsApp| {
                    state.ai.request(Operation::SetDevice(preference));
                },
            )
            .disabled(busy || device == preference),
            components::ButtonSurface::Control,
            device == preference,
        )
        .height(34.0.px())
    };
    section(
        "Processing mode",
        flex_col((
            flex_row((
                option("Automatic (GPU if available)", DevicePreference::Automatic),
                option("CPU only", DevicePreference::Cpu),
            ))
            .gap(8.0.px()),
            note(if device == DevicePreference::Automatic {
                "Current mode: Automatic"
            } else {
                "Current mode: CPU only"
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(8.0.px()),
    )
}

#[cfg(test)]
mod tests;
