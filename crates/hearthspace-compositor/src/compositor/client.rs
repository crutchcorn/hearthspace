use smithay::wayland::compositor::CompositorClientState;
use tracing::debug;
use wayland_server::backend::{ClientData, ClientId, DisconnectReason};

pub(super) fn is_x11_surface(surface: &wayland_server::protocol::wl_surface::WlSurface) -> bool {
    #[cfg(feature = "xwayland")]
    {
        use wayland_server::Resource;
        surface
            .client()
            .and_then(|client| client.get_data::<ClientState>().map(|data| data.x11_bridge))
            .unwrap_or(false)
    }
    #[cfg(not(feature = "xwayland"))]
    {
        let _ = surface;
        false
    }
}

#[derive(Default)]
pub(super) struct ClientState {
    pub(super) compositor_state: CompositorClientState,
    #[cfg(feature = "xwayland")]
    pub(super) x11_bridge: bool,
}

#[cfg(feature = "xwayland")]
impl ClientState {
    pub(super) fn satellite() -> Self {
        Self {
            x11_bridge: true,
            ..Self::default()
        }
    }
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        debug!(?client_id, "Wayland client initialized");
    }

    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        debug!(?client_id, ?reason, "Wayland client disconnected");
    }
}
