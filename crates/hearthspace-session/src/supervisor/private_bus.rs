//! An owned development broker uses the same private policy as the VT harness.
use std::io;

use super::*;
use crate::process::ProcessSpec;

impl Supervisor {
    pub(crate) fn start_session_bus(
        &mut self,
        address: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(address) = address else {
            return Ok(());
        };
        self.check_startup_deadline("session bus")?;
        let mut envs = self.child_env(true);
        envs.extend(crate::profile_env::for_session(&self.session_env)?);
        let spec = ProcessSpec {
            name: "session-bus",
            program: "/usr/bin/dbus-daemon".into(),
            args: vec![
                "--session".into(),
                "--nofork".into(),
                "--nopidfile".into(),
                format!("--address={address}"),
            ],
            envs,
            env_removals: [
                "DISPLAY",
                "WAYLAND_SOCKET",
                "AT_SPI_BUS_ADDRESS",
                "DBUS_SESSION_BUS_PID",
                "DBUS_SESSION_BUS_WINDOWID",
                "DBUS_STARTER_ADDRESS",
                "DBUS_STARTER_BUS_TYPE",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            current_dir: None,
        };
        self.session_bus = Some(ManagedProcess::spawn_parent_bound(spec)?);
        self.record_processes()?;
        self.wait_for_session_bus(address)
    }

    fn wait_for_session_bus(&mut self, address: &str) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            self.check_startup_deadline("session bus")?;
            let deadline = self
                .startup_deadline
                .min(Instant::now() + Duration::from_millis(100));
            let ready = hearthspace_desktop::activation::operations::bounded(
                deadline,
                &self.shutdown_requested,
                async {
                    let connection = zbus::connection::Builder::address(address)
                        .map_err(io::Error::other)?
                        .build()
                        .await
                        .map_err(io::Error::other)?;
                    let proxy = zbus::fdo::DBusProxy::new(&connection)
                        .await
                        .map_err(io::Error::other)?;
                    proxy.get_id().await.map_err(io::Error::other)?;
                    Ok(())
                },
            );
            if ready.is_ok() {
                self.check_startup_deadline("session bus")?;
                info!("private session bus ready");
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub(super) fn check_session_bus(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(status) = poll_process(&mut self.session_bus)? {
            return Err(format!("private session bus exited: {status}").into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
