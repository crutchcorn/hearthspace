use super::*;
use zbus::fdo::ObjectManagerProxy;
const BLUEZ: &str = "org.bluez";
async fn objects(c: &Connection) -> Result<zbus::fdo::ManagedObjects> {
    ObjectManagerProxy::builder(c)
        .destination(BLUEZ)
        .map_err(failure)?
        .path("/")
        .map_err(failure)?
        .build()
        .await
        .map_err(failure)?
        .get_managed_objects()
        .await
        .map_err(failure)
}
async fn adapters(c: &Connection) -> Result<Vec<OwnedObjectPath>> {
    Ok(objects(c)
        .await?
        .into_iter()
        .filter(|(_, v)| v.contains_key("org.bluez.Adapter1"))
        .map(|(p, _)| p)
        .collect())
}
pub(super) async fn read(c: &Connection) -> Result<Radio> {
    let owner = owner_name(c, BLUEZ).await?;
    let mut result = Radio::default();
    let objects = objects(c).await?;
    let mut found = false;
    for (id, interfaces) in objects {
        if interfaces.contains_key("org.bluez.Adapter1") {
            found = true;
            result.enabled |= proxy(c, BLUEZ, id.as_str(), "org.bluez.Adapter1")
                .await?
                .get_property::<bool>("Powered")
                .await
                .map_err(failure)?;
        }
        if interfaces.contains_key("org.bluez.Device1") {
            let p = proxy(c, BLUEZ, id.as_str(), "org.bluez.Device1").await?;
            let connected = p.get_property("Connected").await.map_err(failure)?;
            let paired = p.get_property("Paired").await.map_err(failure)?;
            result.items.push(Item {
                id: format!("{owner}|{id}"),
                name: p.get_property("Alias").await.map_err(failure)?,
                detail: if connected {
                    "Connected"
                } else if paired {
                    "Paired"
                } else {
                    "Not paired"
                }
                .into(),
                connected,
                paired,
            });
        }
    }
    if !found {
        return Err("No Bluetooth adapter is available.".into());
    }
    result.items.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(a.name.cmp(&b.name))
    });
    Ok(result)
}
pub(super) async fn power(c: &Connection, on: bool) -> Result<()> {
    let adapters = adapters(c).await?;
    if adapters.is_empty() {
        return Err("No Bluetooth adapter is available.".into());
    }
    for id in adapters {
        proxy(c, BLUEZ, id.as_str(), "org.bluez.Adapter1")
            .await?
            .set_property("Powered", on)
            .await
            .map_err(failure)?;
    }
    Ok(())
}
pub(super) async fn scan(c: &Connection) -> Result<()> {
    // A separate connection owns only this discovery session. Dropping it also
    // releases discovery on cancellation without stopping another app's scan.
    let mut owned = vec![];
    for id in adapters(c).await? {
        let p = proxy(c, BLUEZ, id.as_str(), "org.bluez.Adapter1").await?;
        if p.get_property::<bool>("Powered").await.map_err(failure)? {
            let _: () = p.call("StartDiscovery", &()).await.map_err(failure)?;
            owned.push(id);
        }
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    for id in owned {
        let _: () = proxy(c, BLUEZ, id.as_str(), "org.bluez.Adapter1")
            .await?
            .call("StopDiscovery", &())
            .await
            .map_err(failure)?;
    }
    Ok(())
}
pub(super) async fn device_call(c: &Connection, id: &str, method: &str) -> Result<()> {
    let destination = id.split_once('|').map(|(owner, _)| owner).unwrap_or(BLUEZ);
    let id = target(c, BLUEZ, id, "/org/bluez/").await?;
    let _: () = proxy(c, destination, id.as_str(), "org.bluez.Device1")
        .await?
        .call(method, &())
        .await
        .map_err(failure)?;
    Ok(())
}
pub(super) async fn forget(c: &Connection, id: &str) -> Result<()> {
    let destination = id.split_once('|').map(|(owner, _)| owner).unwrap_or(BLUEZ);
    let id = target(c, BLUEZ, id, "/org/bluez/").await?;
    let device = proxy(c, destination, id.as_str(), "org.bluez.Device1").await?;
    let adapter: OwnedObjectPath = device.get_property("Adapter").await.map_err(failure)?;
    let _: () = proxy(c, destination, adapter.as_str(), "org.bluez.Adapter1")
        .await?
        .call("RemoveDevice", &(&id,))
        .await
        .map_err(failure)?;
    Ok(())
}
pub(super) use super::agent::pair;
