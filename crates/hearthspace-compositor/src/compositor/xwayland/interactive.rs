//! Keep X11's root coordinates in step with compositor-owned interactions.
use tracing::warn;

use crate::compositor::{App, ManagedSurface};

impl App {
    pub(in crate::compositor) fn valid_x11_pointer_request(
        &self,
        window_index: usize,
        button: u32,
    ) -> bool {
        // EWMH uses X button numbers, not evdev codes. Zero leaves the button
        // unspecified; the live implicit grab still has to be a left press.
        // Other buttons cannot start an operation terminated by left release.
        use crate::compositor::input::policy::{Action, Phase, Source};
        matches!(button, 0 | 1)
            && self.interaction_allowed(
                Action::ClientWindowOperation,
                Source::Client,
                Phase::Continue,
            )
            && self.pointer_grab_targets_window(window_index)
    }

    pub(in crate::compositor) fn sync_x11_window_position(&self, id: u64) {
        let Some(window) = self.windows.iter().find(|window| window.id == id) else {
            return;
        };
        let ManagedSurface::X11(surface) = &window.surface else {
            return;
        };
        if surface.is_override_redirect() {
            return;
        }
        // Preserve the last requested size, including a resize the client has
        // not painted yet. Changing only ManagedWindow.position leaves X11
        // TranslateCoordinates and root-relative client events behind the image.
        let mut geometry = surface.last_configure();
        let position = (window.position.x, window.position.y).into();
        if geometry.loc == position {
            return;
        }
        geometry.loc = position;
        if let Err(error) = surface.configure(Some(geometry)) {
            warn!(%error, window_id = id, "failed to synchronize X11 window position");
        }
    }
}
