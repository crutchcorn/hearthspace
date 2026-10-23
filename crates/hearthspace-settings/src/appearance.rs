//! Persisted appearance changes and a bounded live refresh for an open window.
use hearthspace_ipc::{
    AccentColor, Appearance, ColorMode, Settings, client_get_settings, client_set_accent_color,
    client_set_color_mode,
};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use crate::state::SettingsApp;
use std::sync::atomic::Ordering;

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
            state.settings.appearance = settings.appearance;
            state
                .appearance_theme
                .revision
                .fetch_add(1, Ordering::SeqCst);
            state.status = match crate::settings_backend::notify_compositor_settings_changed() {
                Ok(()) => String::new(),
                Err(error) => format!("Saved appearance; compositor reload failed: {error}"),
            };
        }
        Err(error) => state.status = format!("Failed to save appearance: {error}"),
    }
}

fn apply_refresh(state: &mut SettingsApp, (started_at, appearance): (u64, Appearance)) {
    if started_at == state.appearance_theme.revision.load(Ordering::SeqCst) {
        state.settings.appearance = appearance;
    }
}

pub(crate) fn task() -> impl View<SettingsApp, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<(u64, Appearance)>, state: &mut SettingsApp| {
            let revision = state.appearance_theme.revision.clone();
            async move {
                let mut previous = None;
                loop {
                    let started_at = revision.load(Ordering::SeqCst);
                    let result = xilem::tokio::task::spawn_blocking(client_get_settings).await;
                    if let Ok(Ok(settings)) = result
                        && previous != Some((started_at, settings.appearance))
                    {
                        previous = Some((started_at, settings.appearance));
                        if proxy.message((started_at, settings.appearance)).is_err() {
                            return;
                        }
                    }
                    // The IPC read itself has a 50ms timeout. Transient errors keep
                    // the last successful theme and do not cause repaint loops.
                    xilem::tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        },
        apply_refresh,
    )
}
