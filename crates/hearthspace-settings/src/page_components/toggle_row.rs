//! One shared native switch owns the whole card and its focus stop.
use super::toggle;
use crate::state::SettingsApp;
use hearthspace_ui_components::controls::toggle::Toggle;
pub(crate) fn toggle_row<F>(
    name: &'static str,
    description: Option<&'static str>,
    on: bool,
    disabled: bool,
    callback: F,
) -> Toggle<F>
where
    F: Fn(&mut SettingsApp, bool) + Send + Sync + 'static,
{
    toggle(name, on, disabled, callback).with_row(description)
}
#[cfg(test)]
mod tests;
