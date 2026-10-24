//! Persisted appearance changes and a bounded live refresh for an open window.
use hearthspace_ipc::{
    AccentColor, ColorMode, Settings, client_set_accent_color, client_set_color_mode,
};
use xilem::core::{NoElement, View};

use crate::state::SettingsApp;
#[cfg(test)]
use hearthspace_ipc::{Appearance, client_get_settings};

#[cfg(test)]
mod tests;
mod theme;
pub(crate) use theme::Theme;
pub(crate) use theme::properties;

pub(crate) fn set_mode(state: &mut SettingsApp, mode: ColorMode) {
    if state.settings.appearance.color_mode != mode {
        saved(state, client_set_color_mode(mode));
    }
}

pub(crate) fn set_accent(state: &mut SettingsApp, accent: AccentColor) {
    if state.settings.appearance.accent_color != accent {
        saved(state, client_set_accent_color(accent));
    }
}

fn saved(state: &mut SettingsApp, result: std::io::Result<Settings>) {
    match result {
        Ok(settings) => {
            // A theme response must not replace a monitor drag or another page's
            // pending local edit with a separately read settings snapshot.
            crate::settings_refresh::local_save_finished(state);
            state.settings.appearance = settings.appearance;
            state.status.clear();
        }
        Err(error) => state.status = format!("Failed to save appearance: {error}"),
    }
}

pub(crate) fn task() -> impl View<SettingsApp, (), xilem::ViewCtx, Element = NoElement> + use<> {
    crate::settings_refresh::task()
}
