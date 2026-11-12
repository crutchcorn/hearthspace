//! Settings binds the shared switch to its application state.
use crate::state::SettingsApp;
use hearthspace_ui_components::controls::toggle::Toggle;

pub(crate) fn toggle<F>(name: &'static str, on: bool, disabled: bool, callback: F) -> Toggle<F>
where
    F: Fn(&mut SettingsApp, bool) + Send + Sync + 'static,
{
    hearthspace_ui_components::controls::toggle::toggle(name, on, disabled, callback)
}
