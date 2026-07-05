use smithay::wayland::compositor::CompositorClientState;
use tracing::debug;
use wayland_server::backend::{ClientData, ClientId, DisconnectReason};

#[derive(Default)]
pub(super) struct ClientState {
    pub(super) compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        debug!(?client_id, "Wayland client initialized");
    }

    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        debug!(?client_id, ?reason, "Wayland client disconnected");
    }
}
