use super::*;
use zbus::zvariant::Value;
const NM: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
type Settings = HashMap<String, Properties>;
async fn manager(c: &Connection) -> Result<Proxy<'_>> {
    proxy(c, NM, ROOT, NM).await
}
async fn devices(c: &Connection) -> Result<Vec<OwnedObjectPath>> {
    manager(c)
        .await?
        .call("GetDevices", &())
        .await
        .map_err(failure)
}
async fn wireless(c: &Connection, id: &str) -> Result<bool> {
    let p = proxy(c, NM, id, "org.freedesktop.NetworkManager.Device").await?;
    Ok(p.get_property::<u32>("DeviceType").await.map_err(failure)? == 2)
}
pub(super) async fn read(c: &Connection) -> Result<Radio> {
    let owner = owner_name(c, NM).await?;
    let m = manager(c).await?;
    let mut radio = Radio {
        enabled: m.get_property("WirelessEnabled").await.map_err(failure)?,
        blocked: !m
            .get_property::<bool>("WirelessHardwareEnabled")
            .await
            .map_err(failure)?,
        items: vec![],
    };
    let mut found = false;
    for d in devices(c).await? {
        if !wireless(c, d.as_str()).await? {
            continue;
        }
        found = true;
        let state: u32 = proxy(c, NM, d.as_str(), "org.freedesktop.NetworkManager.Device")
            .await?
            .get_property("State")
            .await
            .map_err(failure)?;
        let p = proxy(c, NM, d.as_str(), WIRELESS).await?;
        let active: OwnedObjectPath = p.get_property("ActiveAccessPoint").await.map_err(failure)?;
        let aps: Vec<OwnedObjectPath> = p.call("GetAccessPoints", &()).await.map_err(failure)?;
        for ap in aps {
            let p = proxy(
                c,
                NM,
                ap.as_str(),
                "org.freedesktop.NetworkManager.AccessPoint",
            )
            .await?;
            let ssid: Vec<u8> = p.get_property("Ssid").await.map_err(failure)?;
            if ssid.is_empty() {
                continue;
            }
            let strength: u8 = p.get_property("Strength").await.map_err(failure)?;
            let secure = p.get_property::<u32>("Flags").await.map_err(failure)? & 1 != 0;
            let connected = ap == active && state == 100;
            radio.items.push(Item {
                id: format!("{owner}|{ap}"),
                name: String::from_utf8_lossy(&ssid).into_owned(),
                detail: format!(
                    "{} · {}%{}",
                    if connected {
                        "Connected"
                    } else if secure {
                        "Secured"
                    } else {
                        "Open"
                    },
                    strength,
                    if radio.blocked {
                        " · Hardware blocked"
                    } else {
                        ""
                    }
                ),
                connected,
                paired: false,
                secured: secure,
            });
        }
    }
    if !found {
        return Err("No Wi-Fi adapter is available.".into());
    }
    radio.items.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(a.name.cmp(&b.name))
            .then(a.id.cmp(&b.id))
    });
    Ok(radio)
}
pub(super) async fn power(c: &Connection, on: bool) -> Result<()> {
    manager(c)
        .await?
        .set_property("WirelessEnabled", on)
        .await
        .map_err(failure)
}
pub(super) async fn scan(c: &Connection) -> Result<()> {
    for d in devices(c).await? {
        if wireless(c, d.as_str()).await? {
            let _: () = proxy(c, NM, d.as_str(), WIRELESS)
                .await?
                .call("RequestScan", &(Properties::new(),))
                .await
                .map_err(failure)?;
        }
    }
    Ok(())
}
async fn owner(c: &Connection, ap: &OwnedObjectPath) -> Result<OwnedObjectPath> {
    for d in devices(c).await? {
        if !wireless(c, d.as_str()).await? {
            continue;
        }
        let aps: Vec<OwnedObjectPath> = proxy(c, NM, d.as_str(), WIRELESS)
            .await?
            .call("GetAccessPoints", &())
            .await
            .map_err(failure)?;
        if aps.contains(ap) {
            return Ok(d);
        }
    }
    Err("This network is no longer available. Refresh the list.".into())
}
pub(super) async fn disconnect(c: &Connection, id: &str) -> Result<()> {
    let ap = target(c, NM, id, "/org/freedesktop/NetworkManager/AccessPoint/").await?;
    let destination = id.split_once('|').expect("validated device identity").0;
    let device = owner(c, &ap).await?;
    let current: OwnedObjectPath = proxy(c, destination, device.as_str(), WIRELESS)
        .await?
        .get_property("ActiveAccessPoint")
        .await
        .map_err(failure)?;
    if current != ap {
        return Err("This network is no longer connected.".into());
    }
    let active: OwnedObjectPath = proxy(
        c,
        destination,
        device.as_str(),
        "org.freedesktop.NetworkManager.Device",
    )
    .await?
    .get_property("ActiveConnection")
    .await
    .map_err(failure)?;
    let specific: OwnedObjectPath = proxy(
        c,
        destination,
        active.as_str(),
        "org.freedesktop.NetworkManager.Connection.Active",
    )
    .await?
    .get_property("SpecificObject")
    .await
    .map_err(failure)?;
    if specific != ap {
        return Err("The active network changed. Refresh before disconnecting.".into());
    }
    let _: () = proxy(c, destination, ROOT, NM)
        .await?
        .call("DeactivateConnection", &(&active,))
        .await
        .map_err(failure)?;
    Ok(())
}
fn value(v: Value<'_>) -> OwnedValue {
    v.try_to_owned().expect("owned connection setting")
}
pub(super) async fn connect(c: &Connection, id: &str) -> Result<()> {
    connect_with_password(c, id, super::prompt::password).await
}
pub(super) async fn connect_with_password<F, Fut>(
    c: &Connection,
    id: &str,
    password: F,
) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    let ap = target(c, NM, id, "/org/freedesktop/NetworkManager/AccessPoint/").await?;
    let destination = id.split_once('|').expect("validated device identity").0;
    let device = owner(c, &ap).await?;
    let a = proxy(
        c,
        destination,
        ap.as_str(),
        "org.freedesktop.NetworkManager.AccessPoint",
    )
    .await?;
    let ssid: Vec<u8> = a.get_property("Ssid").await.map_err(failure)?;
    // Only profiles compatible with this device are considered. NM validates
    // their security and existing secret-agent policy on activation.
    let profiles: Vec<OwnedObjectPath> = proxy(
        c,
        destination,
        device.as_str(),
        "org.freedesktop.NetworkManager.Device",
    )
    .await?
    .get_property("AvailableConnections")
    .await
    .map_err(failure)?;
    for profile in profiles {
        let settings: Settings = proxy(
            c,
            destination,
            profile.as_str(),
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?
        .call("GetSettings", &())
        .await
        .map_err(failure)?;
        let saved = settings
            .get("802-11-wireless")
            .and_then(|v| v.get("ssid"))
            .and_then(|v| Vec::<u8>::try_from(v.try_clone().ok()?).ok());
        if saved.as_ref() == Some(&ssid) {
            target(c, NM, id, "/org/freedesktop/NetworkManager/AccessPoint/").await?;
            let active: OwnedObjectPath = proxy(c, destination, ROOT, NM)
                .await?
                .call("ActivateConnection", &(&profile, &device, &ap))
                .await
                .map_err(failure)?;
            return activated(c, destination, active).await;
        }
    }
    let flags: u32 = a.get_property("Flags").await.map_err(failure)?;
    let rsn: u32 = a.get_property("RsnFlags").await.map_err(failure)?;
    let wpa: u32 = a.get_property("WpaFlags").await.map_err(failure)?;
    let mut settings = Settings::new();
    settings.insert(
        "connection".into(),
        HashMap::from([
            (
                "id".into(),
                value(Value::from(String::from_utf8_lossy(&ssid).to_string())),
            ),
            ("type".into(), value(Value::from("802-11-wireless"))),
            (
                "permissions".into(),
                value(Value::from(vec![format!(
                    "user:{}:",
                    nix::unistd::User::from_uid(nix::unistd::getuid())
                        .map_err(failure)?
                        .ok_or_else(|| "Could not determine the current user.".to_owned())?
                        .name
                )])),
            ),
        ]),
    );
    settings.insert(
        "802-11-wireless".into(),
        HashMap::from([("ssid".into(), value(Value::from(ssid)))]),
    );
    if flags & 1 != 0 {
        let key = if rsn & 0x400 != 0 {
            "sae"
        } else if (rsn | wpa) & 0x100 != 0 {
            "wpa-psk"
        } else {
            return Err("This network needs advanced security settings. Configure a saved NetworkManager connection first.".into());
        };
        let password = password().await?;
        settings.insert(
            "802-11-wireless-security".into(),
            HashMap::from([
                ("key-mgmt".into(), value(Value::from(key))),
                ("psk".into(), value(Value::from(password))),
            ]),
        );
    }
    // Authentication can outlive the daemon which supplied this row.
    target(c, NM, id, "/org/freedesktop/NetworkManager/AccessPoint/").await?;
    let (_, active): (OwnedObjectPath, OwnedObjectPath) = proxy(c, destination, ROOT, NM)
        .await?
        .call("AddAndActivateConnection", &(&settings, &device, &ap))
        .await
        .map_err(failure)?;
    activated(c, destination, active).await
}

pub(super) async fn forget(c: &Connection, id: &str) -> Result<()> {
    let ap = target(c, NM, id, "/org/freedesktop/NetworkManager/AccessPoint/").await?;
    let destination = id.split_once('|').expect("validated device identity").0;
    let device = owner(c, &ap).await?;
    let ssid: Vec<u8> = proxy(
        c,
        destination,
        ap.as_str(),
        "org.freedesktop.NetworkManager.AccessPoint",
    )
    .await?
    .get_property("Ssid")
    .await
    .map_err(failure)?;
    let profiles: Vec<OwnedObjectPath> = proxy(
        c,
        destination,
        device.as_str(),
        "org.freedesktop.NetworkManager.Device",
    )
    .await?
    .get_property("AvailableConnections")
    .await
    .map_err(failure)?;
    for profile in profiles {
        let p = proxy(
            c,
            destination,
            profile.as_str(),
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;
        let settings: Settings = p.call("GetSettings", &()).await.map_err(failure)?;
        let saved = settings
            .get("802-11-wireless")
            .and_then(|v| v.get("ssid"))
            .and_then(|v| Vec::<u8>::try_from(v.try_clone().ok()?).ok());
        if saved.as_ref() == Some(&ssid) {
            target(c, NM, id, "/org/freedesktop/NetworkManager/AccessPoint/").await?;
            let _: () = p.call("Delete", &()).await.map_err(failure)?;
        }
    }
    Ok(())
}
async fn activated(c: &Connection, destination: &str, active: OwnedObjectPath) -> Result<()> {
    for _ in 0..90 {
        let state: u32 = proxy(
            c,
            destination,
            active.as_str(),
            "org.freedesktop.NetworkManager.Connection.Active",
        )
        .await?
        .get_property("State")
        .await
        .map_err(failure)?;
        match state {
            2 => return Ok(()),
            3 | 4 => {
                return Err("Could not connect. Check the network password and try again.".into());
            }
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
    Err(
        "The connection is taking longer than expected. Check its status before trying again."
            .into(),
    )
}
