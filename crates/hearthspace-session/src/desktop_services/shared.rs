//! Standard desktop portals belong to the systemd user manager, not our session.
use std::{
    io,
    path::Path,
    process::Command,
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant},
};

use crate::{bounded_command, readiness::remaining};

use super::owner_pid;

// Data services must keep the user's ordinary permission database and document
// mount. Only display-dependent portals need refreshing for this desktop.
const DATA_SERVICES: &[(&str, &str, &str)] = &[
    (
        "permission-store",
        "xdg-permission-store.service",
        "org.freedesktop.impl.portal.PermissionStore",
    ),
    (
        "document-portal",
        "xdg-document-portal.service",
        "org.freedesktop.portal.Documents",
    ),
];
const DISPLAY_SERVICES: &[(&str, &str, &str)] = &[
    (
        "portal-gtk",
        "xdg-desktop-portal-gtk.service",
        "org.freedesktop.impl.portal.desktop.gtk",
    ),
    (
        "desktop-portal",
        "xdg-desktop-portal.service",
        "org.freedesktop.portal.Desktop",
    ),
];

#[derive(Default)]
pub(super) struct SharedServices {
    // Diagnostic snapshots only: never turn these PIDs into owned processes.
    entries: Vec<(&'static str, u32)>,
}

impl SharedServices {
    pub(super) fn start_with_systemctl(
        &mut self,
        systemctl: &Path,
        host_runtime: &Path,
        bus: &str,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> io::Result<()> {
        let host_runtime = host_runtime.canonicalize()?;
        for (services, action) in [(DATA_SERVICES, "start"), (DISPLAY_SERVICES, "restart")] {
            for &(name, unit, bus_name) in services {
                let service_deadline = deadline.min(Instant::now() + Duration::from_secs(5));
                if action == "start"
                    && let Some(pid) = owner_pid(bus, bus_name, service_deadline, cancelled)?
                {
                    self.entries.push((name, pid));
                    continue;
                }
                // Separate awaited commands preserve GTK-before-frontend order;
                // the installed units do not declare ordering between portals.
                let result = bounded_command::output(
                    Command::new(systemctl)
                        .args(["--user", "--no-pager", action, unit])
                        .env("XDG_RUNTIME_DIR", &host_runtime)
                        .env("DBUS_SESSION_BUS_ADDRESS", bus)
                        .env_remove("DBUS_STARTER_ADDRESS")
                        .env_remove("DBUS_STARTER_BUS_TYPE"),
                    service_deadline,
                    cancelled,
                )?;
                if !result.status.success() {
                    return Err(io::Error::other(format!(
                        "could not {action} {unit}: {}",
                        result.status
                    )));
                }
                let pid = loop {
                    remaining(service_deadline, cancelled)?;
                    if let Some(pid) = owner_pid(bus, bus_name, service_deadline, cancelled)? {
                        break pid;
                    }
                    thread::sleep(
                        remaining(service_deadline, cancelled)?.min(Duration::from_millis(10)),
                    );
                };
                self.entries.push((name, pid));
            }
        }
        Ok(())
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn process_json_fields(&self) -> String {
        self.entries
            .iter()
            .map(|(name, pid)| format!(",\"{name}\":{pid}"))
            .collect()
    }
}

#[cfg(test)]
mod tests;
