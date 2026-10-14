//! Private bus fixtures for serialized integration tests. Accessibility services
//! inherit their own runtime directory, so they cannot replace the host socket.

use std::{
    ffi::OsString,
    io::BufRead,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

pub struct PrivateSessionBus {
    previous_address: Option<OsString>,
    previous_atspi: Option<OsString>,
    child: Child,
    _runtime: tempfile::TempDir,
}

impl PrivateSessionBus {
    /// The caller must hold its integration-test environment lock until Drop.
    pub fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = tempfile::tempdir()?;
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env_remove("AT_SPI_BUS_ADDRESS")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().expect("stdout was piped");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut address = String::new();
            let result = std::io::BufReader::new(stdout)
                .read_line(&mut address)
                .map(|_| address.trim().to_owned());
            let _ = sender.send(result);
        });
        let address = match receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(address)) if !address.is_empty() => address,
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("private D-Bus startup failed: {result:?}").into());
            }
        };
        let previous_address = std::env::var_os("DBUS_SESSION_BUS_ADDRESS");
        let previous_atspi = std::env::var_os("AT_SPI_BUS_ADDRESS");
        // SAFETY: each integration target serializes its tests with a lock held
        // through this guard's Drop, including all child-process creation.
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address);
            std::env::remove_var("AT_SPI_BUS_ADDRESS");
        }
        Ok(Self {
            previous_address,
            previous_atspi,
            child,
            _runtime: runtime,
        })
    }
}

impl Drop for PrivateSessionBus {
    fn drop(&mut self) {
        for (key, value) in [
            ("DBUS_SESSION_BUS_ADDRESS", &self.previous_address),
            ("AT_SPI_BUS_ADDRESS", &self.previous_atspi),
        ] {
            // SAFETY: the caller's integration-test lock is still held.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub async fn enable_accessibility() -> Result<(), Box<dyn std::error::Error>> {
    let connection = atspi::zbus::Connection::session().await?;
    let status = atspi::proxy::bus::StatusProxy::new(&connection).await?;
    status.set_screen_reader_enabled(true).await?;
    Ok(())
}
