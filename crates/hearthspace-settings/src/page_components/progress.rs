//! Preserve backend fractions and unknown download progress through shared UI.
use crate::state::SettingsApp;
pub(crate) fn progress(value: Option<f64>) -> impl xilem::WidgetView<SettingsApp> {
    hearthspace_ui_components::controls::progress::progress("AI model progress", value)
}
