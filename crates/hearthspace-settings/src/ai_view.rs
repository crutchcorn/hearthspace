use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::{
        layout::AsUnit,
        parley::style::{FontWeight, LineHeight},
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_item, flex_row, sized_box},
};

use crate::{
    ai::Operation,
    page_components::{disclosure, progress, restore_focus_after_close, toggle_row},
    state::SettingsApp,
    typography::label,
};

mod confirmation;
mod status;

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let ai = &state.ai;
    let busy = ai.pending.is_some();
    let enabled = ai.enabled();
    confirmation::modal(flex_col((
        flex_col((
            restore_focus_after_close(
                toggle_row(
                    "Enable AI features",
                    None,
                    enabled,
                    busy || ai.confirm_off || (ai.snapshot.is_none() && !ai.needs_retry()),
                    |state, on| state.ai.set_enabled(on),
                ),
                ai.confirm_off || ai.pending == Some(Operation::Remove),
            ),
            (!enabled && !busy).then(|| {
                detail(palette, if ai.installed() {
                    "Turn on AI to use the local model."
                } else {
                    "Download the local model to get started."
                })
            }),
            status::status(
                flex_col((
                    detail(palette, ai.message.clone()),
                    busy.then(|| progress(ai.progress)),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(8.0.px())
                .padding(Padding {
                    top: 6.0.px(),
                    bottom: 3.0.px(),
                    left: 0.0.px(),
                    right: 0.0.px(),
                }),
            ),
            (!busy && !ai.confirm_off && ai.needs_retry()).then(|| {
                action(palette, "Try again", components::SettingsButton::Control, |state| {
                    state.ai.retry();
                })
            }),
            (!busy && !ai.confirm_off && !enabled && ai.can_remove()
                && (ai.needs_retry() || ai.snapshot.as_ref().is_some_and(|status| {
                    status.downloaded && !status.installed
                }))).then(|| {
                action(palette,
                    if ai.installed() {
                        "Remove local model"
                    } else {
                        "Remove incomplete download"
                    },
                    components::SettingsButton::Bare,
                    |state| state.ai.confirm_off = true,
                )
            }),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(10.0.px()),
        flex_col((
            label("Features")
                .text_size(11.0)
                .weight(FontWeight::MEDIUM)
                .color(palette.muted),
            flex_col((
                feature(palette,
                    "On-device AI",
                    "AI runs locally on your device. Window titles are processed on this device. Installing the model requires a download.",
                    0,
                    ai.features_open[0],
                ),
                feature(palette,
                    "Gather similar windows",
                    "Select one or more windows, then choose Gather similar windows. AI finds related windows on your canvas and brings them into view beside your selection.",
                    1,
                    ai.features_open[1],
                ),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(10.0.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(22.0.px()), ai.confirm_off)
    .boxed()
}

fn action(
    palette: components::Palette,
    name: &'static str,
    surface: components::SettingsButton,
    callback: impl Fn(&mut SettingsApp) + Send + Sync + 'static,
) -> impl WidgetView<SettingsApp> {
    components::settings_button(
        button(
            label(name)
                .text_size(11.0)
                .color(if surface == components::SettingsButton::Primary {
                    palette.accent_ink
                } else {
                    palette.text
                }),
            callback,
        )
        .height(40.0.px())
        .padding(Padding::from_vh(7.0.px(), 12.0.px())),
        surface,
        false,
    )
}

fn detail(palette: components::Palette, text: impl Into<String>) -> impl WidgetView<SettingsApp> {
    label(text.into())
        .text_size(10.0)
        .line_height(LineHeight::FontSizeRelative(1.6))
        .line_break_mode(LineBreaking::WordWrap)
        .color(palette.muted)
}

fn feature(
    palette: components::Palette,
    name: &'static str,
    description: &'static str,
    index: usize,
    open: bool,
) -> impl WidgetView<SettingsApp> {
    components::settings_surface(
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
                        .color(palette.muted),
                    ))
                    .gap(12.0.px()),
                    move |state: &mut SettingsApp| {
                        state.ai.features_open[index] = !state.ai.features_open[index];
                    },
                )
                .height(44.0.px())
                .padding(12.0.px()),
                open.then(|| {
                    sized_box(
                        label(description)
                            .text_size(11.0)
                            .line_height(LineHeight::FontSizeRelative(1.65))
                            .line_break_mode(LineBreaking::WordWrap)
                            .color(palette.muted),
                    )
                    .padding(Padding {
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
        .padding(1.0.px()),
        components::SettingsSurface::Subtle,
    )
}

#[cfg(test)]
mod tests;

pub(crate) fn theme_properties(
    properties: &mut xilem::masonry::core::DefaultProperties,
    palette: hearthspace_ui_components::Palette,
) {
    confirmation::theme_properties(properties, palette);
}
