use hearthspace_ipc::client_set_test;
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{CrossAxisAlignment, checkbox, flex_col},
};

use crate::{
    monitors::settings_with_live_outputs,
    page_components::{card, note, section},
    state::SettingsApp,
};

pub(crate) fn view(state: &SettingsApp) -> Box<AnyWidgetView<SettingsApp>> {
    section(
        "Diagnostics",
        card(
            flex_col((
                note("Check the connection to the settings service."),
                checkbox(
                    "Test setting",
                    state.settings.test,
                    |state: &mut SettingsApp, checked| {
                        let previous = state.settings.test;
                        state.settings.test = checked;
                        match client_set_test(checked) {
                            Ok(settings) => {
                                let saved = settings.test;
                                state.settings =
                                    settings_with_live_outputs(settings, &state.live_outputs);
                                state.status = format!("Saved test = {saved}");
                            }
                            Err(error) => {
                                state.settings.test = previous;
                                state.status = format!("Failed to save setting: {error}");
                            }
                        }
                    },
                ),
                note(state.config_label.clone()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(12.0.px()),
        ),
    )
    .boxed()
}
