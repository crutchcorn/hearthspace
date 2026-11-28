//! Standard system-bus radios. Reads never scan, connect, or change radio state.
use std::{collections::HashMap, time::Duration};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, OwnedValue},
};
pub mod bluetooth;
mod prompt;
pub mod wifi;

pub type Result<T> = std::result::Result<T, String>;
pub type Properties = HashMap<String, OwnedValue>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub connected: bool,
    pub paired: bool,
    pub secured: bool,
    /// NetworkManager signal strength, 0–100; absent for Bluetooth.
    pub strength: Option<u8>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Radio {
    pub enabled: bool,
    pub blocked: bool,
    pub items: Vec<Item>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub wifi: Result<Radio>,
    pub bluetooth: Result<Radio>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            wifi: Err("Loading Wi-Fi…".into()),
            bluetooth: Err("Loading Bluetooth…".into()),
        }
    }
}
// Deliberately excludes Debug: commands may eventually carry credential material.
#[derive(Clone)]
pub enum Command {
    Refresh,
    ScanWifi,
    ScanBluetooth,
    WifiPower(bool),
    BluetoothPower(bool),
    ConnectWifi(String),
    DisconnectWifi(String),
    ForgetWifi(String),
    PairBluetooth(String),
    ConnectBluetooth(String),
    DisconnectBluetooth(String),
    ForgetBluetooth(String),
}
pub struct Service {
    connection: Connection,
}
impl Service {
    pub async fn system() -> Result<Self> {
        let connection = tokio::time::timeout(Duration::from_secs(8), Connection::system())
            .await
            .map_err(|_| "System wireless services did not respond.".to_owned())?
            .map_err(|_| "System wireless services are unavailable.".to_owned())?;
        Ok(Self { connection })
    }
    pub fn with_connection(connection: Connection) -> Self {
        Self { connection }
    }
    pub async fn snapshot(&self) -> Snapshot {
        Snapshot {
            wifi: bounded(wifi::read(&self.connection)).await,
            bluetooth: bounded(bluetooth::read(&self.connection)).await,
        }
    }
    pub async fn execute(&self, command: Command) -> Result<()> {
        use Command::*;
        match command {
            Refresh => Ok(()),
            ScanWifi => wifi::scan(&self.connection).await,
            ScanBluetooth => bluetooth::scan(&self.connection).await,
            WifiPower(on) => wifi::power(&self.connection, on).await,
            BluetoothPower(on) => bluetooth::power(&self.connection, on).await,
            ConnectWifi(id) => wifi::connect(&self.connection, &id).await,
            DisconnectWifi(id) => wifi::disconnect(&self.connection, &id).await,
            ForgetWifi(id) => wifi::forget(&self.connection, &id).await,
            PairBluetooth(id) => bluetooth::pair(&self.connection, &id).await,
            ConnectBluetooth(id) => bluetooth::device_call(&self.connection, &id, "Connect").await,
            DisconnectBluetooth(id) => {
                bluetooth::device_call(&self.connection, &id, "Disconnect").await
            }
            ForgetBluetooth(id) => bluetooth::forget(&self.connection, &id).await,
        }
    }
}
async fn bounded<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(8), future)
        .await
        .map_err(|_| "Wireless service did not respond. Try again.".to_owned())?
}
async fn proxy<'a>(
    c: &'a Connection,
    service: &'a str,
    path: &'a str,
    interface: &'a str,
) -> Result<Proxy<'a>> {
    Proxy::new(c, service, path, interface)
        .await
        .map_err(|_| "Wireless service is unavailable.".to_owned())
}
fn failure(_: impl std::fmt::Display) -> String {
    "The request could not be completed. Check the device and your permissions, then try again."
        .into()
}
fn path(id: &str, prefix: &str) -> Result<OwnedObjectPath> {
    if !id.starts_with(prefix) {
        return Err("This wireless device is no longer available.".into());
    }
    OwnedObjectPath::try_from(id.to_owned()).map_err(failure)
}

pub mod state;

async fn owner_name(c: &Connection, service: &str) -> Result<String> {
    proxy(
        c,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?
    .call("GetNameOwner", &(service,))
    .await
    .map_err(failure)
}
async fn target(c: &Connection, service: &str, id: &str, prefix: &str) -> Result<OwnedObjectPath> {
    let (owner, id) = id
        .split_once('|')
        .ok_or_else(|| "Refresh the device list before trying again.".to_owned())?;
    if owner_name(c, service).await? != owner {
        return Err("Wireless service restarted. Refresh the device list.".into());
    }
    path(id, prefix)
}

mod agent;

#[cfg(test)]
mod tests;
