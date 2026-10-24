//! Keep XWM surface hooks on the same state type as the Wayland globals.
use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    time::Duration,
};

use calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use smithay::xwayland::X11Wm;
use tracing::warn;
use wayland_server::{Client, DisplayHandle};

use crate::compositor::App;

impl App {
    pub(super) fn start_xwm(
        &mut self,
        display: &DisplayHandle,
        socket: UnixStream,
        client: Client,
        display_number: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Smithay installs surface commit hooks parameterized by the XWM's
        // state. That must be App, just like CompositorState::new::<App>.
        // Forwarding from CalloopData is insufficient: its TypeId differs.
        let mut events: EventLoop<'static, App> = EventLoop::try_new()?;
        let xwm = X11Wm::start_wm(events.handle(), display, socket, client)?;
        let poll_fd = events.as_fd().try_clone_to_owned()?;
        self.loop_handle.insert_source(
            Generic::new(poll_fd, Interest::READ, Mode::Level),
            move |_, _, data| {
                // Pinned Smithay uses fd-backed sources here (channel, ping,
                // and clipboard pipes). XSync timeouts arrive as X11 events;
                // there are no separate calloop timer deadlines to propagate.
                if let Err(error) = events.dispatch(Duration::ZERO, &mut data.state) {
                    warn!(%error, "failed to dispatch XWayland events");
                    if let Some(xwm) = data.state.xwm.as_ref() {
                        data.state.xwayland_disconnected(xwm.id());
                    }
                }
                if data.state.xdisplay.is_none() {
                    // Keep the XWM alive until this batch finishes: Smithay's
                    // pending transfer callbacks still call xwm_state(). Stop
                    // dispatching this loop before retiring that state.
                    data.state.xwm.take();
                    return Ok(PostAction::Remove);
                }
                Ok(PostAction::Continue)
            },
        )?;
        self.xwm = Some(xwm);
        self.xdisplay = Some(display_number);
        self.publish_xwayland_display();
        self.publish_xsettings();
        Ok(())
    }

    pub(in crate::compositor) fn publish_xsettings(&mut self) {
        let Some(xwm) = self.xwm.as_mut() else { return };
        let theme = match self.appearance.color_mode {
            hearthspace_ipc::ColorMode::Light => "Adwaita",
            hearthspace_ipc::ColorMode::Dark => "Adwaita-dark",
        };
        // Smithay owns the selection, serialization, change serial and property
        // notifications. Modern accent preferences are published by the portal;
        // XSettings has no interoperable GTK accent-color key.
        if let Err(error) =
            xwm.set_xsettings([("Net/ThemeName".into(), theme.to_owned().into())].into_iter())
        {
            warn!(%error, "failed to publish the XWayland application theme");
        }
    }
}
