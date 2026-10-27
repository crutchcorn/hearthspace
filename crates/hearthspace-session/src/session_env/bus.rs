//! Installed native desktops reuse login services; development owns a private bus.
use std::{env, error::Error};

use zbus::address::transport::{Unix, UnixSocket};

use super::SessionPaths;
use crate::{
    cli::{Args, RunMode},
    compositor::is_native_session,
};

pub(crate) fn automatic_private(args: &Args, mode: RunMode) -> bool {
    mode == RunMode::DevCargo && is_native_session(args) && !args.private_session_bus
}

pub(super) fn selected_address(
    args: &Args,
    paths: &SessionPaths,
    owned_address: Option<&str>,
) -> Result<Option<String>, Box<dyn Error>> {
    if let Some(address) = owned_address {
        if !args.private_session_bus {
            return Err("an owned session bus requires private session policy".into());
        }
        return Ok(Some(address.into()));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn native_development_is_private_even_without_desktop_services() {
        let args = Args::parse_from([
            "session",
            "--dev",
            "--backend",
            "native",
            "--no-desktop-services",
        ]);
        assert!(automatic_private(&args, RunMode::DevCargo));
    }

    #[test]
    fn installed_native_keeps_the_canonical_user_bus() {
        let args = Args::parse_from(["session", "--installed", "--backend", "native"]);
        assert!(!automatic_private(&args, RunMode::InstalledBinaries));
    }

    #[test]
    fn explicit_private_bus_is_never_replaced() {
        let args = Args::parse_from([
            "session",
            "--dev",
            "--backend",
            "native",
            "--private-session-bus",
        ]);
        assert!(!automatic_private(&args, RunMode::DevCargo));
    }

    #[test]
    fn nested_development_does_not_start_a_broker() {
        let args = Args::parse_from(["session", "--dev", "--backend", "nested"]);
        assert!(!automatic_private(&args, RunMode::DevCargo));
    }
}
