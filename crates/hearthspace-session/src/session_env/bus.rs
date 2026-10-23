//! Native desktops reuse the user's login services; isolation is explicit.
use std::{env, error::Error};

use zbus::address::transport::{Unix, UnixSocket};

use super::SessionPaths;
use crate::{cli::Args, compositor::is_native_session};

pub(super) fn selected_address(
    args: &Args,
    paths: &SessionPaths,
) -> Result<Option<String>, Box<dyn Error>> {
    if args.private_session_bus {
        let address = env::var("DBUS_SESSION_BUS_ADDRESS")
            .map_err(|_| "--private-session-bus requires an existing dedicated bus")?;
        if address.is_empty() {
            return Err("--private-session-bus requires a nonempty bus address".into());
        }
        return Ok(Some(address));
    }
    Ok(is_native_session(args).then(|| {
        // Inherited addresses can belong to a display manager or another DE.
        // The canonical user bus is where PAM/systemd started the login keyring.
        Unix::new(UnixSocket::File(paths.host_runtime_dir.join("bus"))).to_string()
    }))
}
