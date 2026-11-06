//! Opt-in Satellite runtime while the X11 recipient regressions gate adoption.
use std::{
    io,
    os::{
        fd::AsFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    process::{Command, Stdio},
    sync::Arc,
};

use calloop::{Interest, Mode, PostAction, generic::Generic};
use rustix::{
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    io::Errno,
};
use tracing::{info, warn};
use wayland_server::DisplayHandle;

use crate::compositor::{App, client::ClientState};

mod clipboard;
pub(in crate::compositor) mod interaction;
mod process;
mod settings;
pub(in crate::compositor) use process::Satellite;

impl App {
    pub(super) fn start_satellite(&mut self, dh: &DisplayHandle) -> io::Result<()> {
        let binary = std::env::var_os("HEARTHSPACE_SATELLITE_BINARY")
            .map(std::path::PathBuf::from)
            .unwrap_or(std::env::current_exe()?.with_file_name("hearthspace-satellite"));
        let (server, peer) = UnixStream::pair()?;
        let client = dh
            .clone()
            .insert_client(server, Arc::new(ClientState::default()))
            .map_err(io::Error::other)?;
        self.satellite_interaction
            .advertise(dh, client.id(), &self.primary_selection_state);
        let mut command = Command::new(binary);
        command
            .arg("--embedded")
            .stdin(Stdio::from(std::os::fd::OwnedFd::from(peer)))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .process_group(0);
        let mut satellite = Satellite::spawn(&mut command)?;
        let stdout = satellite.take_stdout();
        fcntl_setfl(&stdout, fcntl_getfl(&stdout)? | OFlags::NONBLOCK)?;
        let mut buffer = Vec::new();
        let mut ready = false;
        self.loop_handle
            .insert_source(
                Generic::new(stdout, Interest::READ, Mode::Level),
                move |_, source, data| {
                    loop {
                        let mut bytes = [0u8; 128];
                        match rustix::io::read(source.as_fd(), &mut bytes) {
                            Ok(0) => {
                                data.state.satellite_interaction.revoke();
                                data.state.xdisplay = None;
                                data.state.publish_xwayland_display();
                                warn!("Satellite readiness pipe closed");
                                return Ok(PostAction::Remove);
                            }
                            Ok(n) => {
                                buffer.extend_from_slice(&bytes[..n]);
                                if ready || buffer.len() > 128 {
                                    warn!("invalid extra Satellite readiness data");
                                    return Ok(PostAction::Remove);
                                }
                                if buffer.ends_with(b"\n") {
                                    let display = std::str::from_utf8(&buffer)
                                        .ok()
                                        .and_then(|line| line.split_whitespace().next())
                                        .and_then(|display| display.strip_prefix(':'))
                                        .and_then(|display| display.parse::<u32>().ok());
                                    let Some(display_number) = display else {
                                        warn!("invalid Satellite readiness record");
                                        return Ok(PostAction::Remove);
                                    };
                                    data.state.xdisplay = Some(display_number);
                                    data.state.publish_xwayland_display();
                                    ready = true;
                                    info!(display_number, "Satellite is ready");
                                }
                            }
                            Err(Errno::AGAIN) => return Ok(PostAction::Continue),
                            Err(error) => return Err(io::Error::from(error)),
                        }
                    }
                },
            )
            .map_err(io::Error::other)?;
        self.satellite = Some(satellite);
        Ok(())
    }
}
