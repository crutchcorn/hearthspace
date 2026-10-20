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
    page_components::{
        card, disclosure, focus_on_mount, note, restore_focus_after_close, section, toggle,
    },
    state::SettingsApp,
};

mod status;

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let ai = &state.ai;
    let busy = ai.pending.is_some();
    let enabled = ai.enabled();
    flex_col((
        flex_col((
            card(
                flex_row((
                    flex_item(label("Enable AI features").text_size(11.0), 1.0),
                    restore_focus_after_close(
                        toggle(
                            "Enable AI features",
                            enabled,
                            busy || ai.confirm_off || (ai.snapshot.is_none() && !ai.needs_retry()),
                            |state, on| state.ai.set_enabled(on),
                        ),
                        ai.confirm_off || ai.pending == Some(Operation::Remove),
                    ),
                ))
                .height(40.0.px())
                .gap(12.0.px()),
            ),
            (!enabled && !busy).then(|| {
                note(if ai.installed() {
                    "Turn on AI to use the local model."
                } else {
                    "Download the local model to get started."
                })
            }),
            status::status(
                flex_col((
                    note(ai.message.clone()),
                    busy.then(|| progress_bar(ai.progress)),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(8.0.px()),
            ),
            (!busy && !ai.confirm_off && ai.needs_retry()).then(|| {
                action("Try again", components::ButtonSurface::Control, |state| {
                    state.ai.retry();
                })
            }),
            (!busy && !ai.confirm_off && !enabled && ai.can_remove()
                && (ai.needs_retry() || ai.snapshot.as_ref().is_some_and(|status| {
                    status.downloaded && !status.installed
                }))).then(|| {
                action(
                    if ai.installed() {
                        "Remove local model"
                    } else {
                        "Remove incomplete download"
                    },
                    components::ButtonSurface::Bare,
                    |state| state.ai.confirm_off = true,
                )
            }),
            ai.confirm_off.then(confirmation),
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
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(22.0.px())
    .boxed()
}

fn action(
    name: &'static str,
    surface: components::ButtonSurface,
    callback: impl Fn(&mut SettingsApp) + Send + Sync + 'static,
) -> impl WidgetView<SettingsApp> {
    components::button_surface(
        button(
            label(name)
                .text_size(11.0)
                .color(if surface == components::ButtonSurface::Primary {
                    components::SURFACE
                } else {
                    components::TEXT
                }),
            callback,
        ),
        surface,
        false,
    )
    .height(40.0.px())
    .padding(Padding::from_vh(7.0.px(), 12.0.px()))
}

fn confirmation() -> impl WidgetView<SettingsApp> {
    card(
        flex_col((
            label("Turn off AI?").text_size(20.0),
            note("Turning off AI deletes the local model and its download. You’ll need to download it again to turn AI back on."),
            flex_row((
                focus_on_mount(action(
                    "Keep AI",
                    components::ButtonSurface::Control,
                    |state| state.ai.confirm_off = false,
                )),
                action(
                    "Turn off and delete",
                    components::ButtonSurface::Primary,
                    |state| state.ai.confirm_removal(),
                ),
            ))
            .gap(8.0.px()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(14.0.px()),
    )
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

#[cfg(test)]
mod tests;
