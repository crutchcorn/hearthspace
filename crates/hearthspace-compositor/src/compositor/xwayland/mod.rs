//! X11 compatibility is owned exclusively by the private Satellite process.
use super::App;
use tracing::error;
use wayland_server::DisplayHandle;

pub(super) mod satellite;

impl App {
    pub(super) fn start_xwayland(&mut self, dh: &DisplayHandle) {
        if let Err(error) = self.start_satellite(dh) {
            error!(%error, "failed to start Satellite; X11 applications are unavailable");
        }
    }
}
