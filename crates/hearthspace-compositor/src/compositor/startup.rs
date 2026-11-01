//! Keep normal clients behind dependency and XWM initialization.
use std::{os::unix::net::UnixListener, sync::Arc};

use calloop::{Interest, Mode, PostAction, generic::Generic};
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{debug, error, info};

use super::{
    App, CalloopData, ClientState, accept_command_connections, command_socket_path,
    remove_stale_socket, spawn_shell,
};
use crate::RunOptions;

pub(super) fn preflight(options: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    if options.start_xwayland {
        #[cfg(feature = "xwayland")]
        super::xwayland::dependency::Dependency::current()?.validate()?;
        #[cfg(not(feature = "xwayland"))]
        return Err("Xwayland support is not compiled in; rebuild with xwayland or explicitly pass --no-xwayland".into());
    }
    Ok(())
}

pub(super) fn start_clients(data: &mut CalloopData) -> Result<(), Box<dyn std::error::Error>> {
    let command_path = command_socket_path();
    remove_stale_socket(&command_path)?;
    let command_listener = UnixListener::bind(&command_path)?;
    command_listener.set_nonblocking(true)?;
    let command_loop_handle = data.state.loop_handle.clone();
    data.state.loop_handle.insert_source(
        Generic::new(command_listener, Interest::READ, Mode::Level),
        move |_, listener, _| {
            accept_command_connections(listener, &command_loop_handle)?;
            Ok(PostAction::Continue)
        },
    )?;
    info!(path = %command_path.display(), "listening for shell commands");
    // Xwayland has its own private Wayland socket and can initialize before
    // this listener exists. Merely binding a public Wayland socket must never
    // release the supervisor or clients before the XWM is usable.
    let wayland_display = crate::config::wayland_display_name();
    let socket_source = ListeningSocketSource::with_name(&wayland_display)?;
    data.state
        .loop_handle
        .insert_source(socket_source, |stream, _, data| {
            match data
                .display
                .handle()
                .insert_client(stream, Arc::new(ClientState::default()))
            {
                Ok(client_id) => debug!(?client_id, "accepted Wayland client"),
                Err(error) => error!(%error, "failed to insert Wayland client"),
            }
            data.state.request_redraw();
        })?;
    data.state.application_launch_ready = true;
    if let Some(path) = data.state.startup_shell.take() {
        info!(path = %path.display(), "spawning shell client");
        spawn_shell(&path);
    }
    Ok(())
}

impl App {
    pub(in crate::compositor) fn require_application_launch_ready(&self) -> Result<(), String> {
        if self.application_launch_ready {
            Ok(())
        } else {
            Err("applications cannot launch until compositor startup completes".into())
        }
    }

    pub(super) fn xwayland_is_ready(&self) -> bool {
        #[cfg(feature = "xwayland")]
        {
            self.xwayland_ready()
        }
        #[cfg(not(feature = "xwayland"))]
        {
            false
        }
    }
}

#[cfg(all(test, not(feature = "xwayland")))]
mod tests {
    use super::*;

    #[test]
    fn build_without_xwayland_still_requires_explicit_wayland_only_mode() {
        let error = preflight(RunOptions::default()).unwrap_err();
        assert!(error.to_string().contains("--no-xwayland"));
        preflight(RunOptions {
            start_xwayland: false,
            ..RunOptions::default()
        })
        .unwrap();
    }
}
