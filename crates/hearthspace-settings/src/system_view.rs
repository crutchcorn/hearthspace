use hearthspace_ipc::client_set_test;
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, button, checkbox, flex_col, flex_item, flex_row, label},
};

use crate::{
    monitors::settings_with_live_outputs,
    page_components::{card, note, section, toggle},
    state::SettingsApp,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    flex_col((
        section(
            "Frame layout",
            flex_col((
                note("Arrange and group your frame items, or change how much detail they show."),
                unavailable_action("Change layout", Icon::FrameLayout),
                note("Layout editing is not available yet."),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(8.0.px()),
        ),
        section(
            "Canvas tools",
            flex_col((
                card(
                    flex_row((
                        flex_item(
                            flex_col((
                                label("Make canvas tools hide-able").text_size(11.0),
                                note("Hiding canvas tools is not available yet."),
                            ))
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .gap(5.0.px()),
                            1.0,
                        ),
                        toggle("Make canvas tools hide-able", false, true, |_, _| {}),
                    ))
                    .gap(16.0.px()),
                ),
                unavailable_action("Restore canvas tools", Icon::RotateCcw),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(8.0.px()),
        ),
        section(
            "Diagnostics",
            card(
                flex_col((
                    note("Check the connection to the settings service."),
                    checkbox("Test setting", state.settings.test, save_test),
                    note(state.config_label.clone()),
                ))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(12.0.px()),
            ),
        ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(25.0.px())
    .boxed()
}

fn unavailable_action(name: &'static str, glyph: Icon) -> impl WidgetView<SettingsApp> {
    components::button_surface(
        button(
            flex_row((
                icon(glyph, 14.0).color(components::TEXT_MUTED),
                label(name).text_size(11.0).color(components::TEXT_MUTED),
            ))
            .gap(7.0.px()),
            |_: &mut SettingsApp| {},
        )
        .disabled(true),
        components::ButtonSurface::Bare,
        false,
    )
    .height(36.0.px())
}

fn save_test(state: &mut SettingsApp, checked: bool) {
    let previous = state.settings.test;
    state.settings.test = checked;
    match client_set_test(checked) {
        Ok(settings) => {
            let saved = settings.test;
            state.settings = settings_with_live_outputs(settings, &state.live_outputs);
            state.status = format!("Saved test = {saved}");
        }
        Err(error) => {
            state.settings.test = previous;
            state.status = format!("Failed to save setting: {error}");
        }
    }
}
