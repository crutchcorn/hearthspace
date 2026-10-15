//! A bounded watcher for the shell-owned bus; an existing watcher is respected.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use futures_util::{StreamExt, stream};
use zbus::{Connection, interface, message::Header, object_server::SignalEmitter};

pub(super) const WATCHER_NAME: &str = "org.kde.StatusNotifierWatcher";
pub(super) const WATCHER_PATH: &str = "/StatusNotifierWatcher";
pub(super) const MAX_ITEMS: usize = 64;
pub(super) type Registry = Arc<Mutex<BTreeMap<Address, BTreeSet<String>>>>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Address {
    pub owner: String,
    pub path: String,
}

impl Address {
    pub fn registered_name(&self) -> String {
        format!("{}{}", self.owner, self.path)
    }
}

pub(super) struct Watcher {
    pub items: Registry,
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().ok_or_else(|| invalid("Missing sender"))?;
        let address = resolve(connection, service, Some(sender.as_str())).await?;
        let source = if service.starts_with('/') {
            address.owner.clone()
        } else {
            service
                .split('/')
                .next()
                .expect("split has a first item")
                .into()
        };
        let added = {
            let mut items = self.items.lock().expect("tray registry poisoned");
            if let Some(sources) = items.get_mut(&address) {
                if sources.len() >= 8 && !sources.contains(&source) {
                    return Err(zbus::fdo::Error::LimitsExceeded(
                        "Too many names for a tray item".into(),
                    ));
                }
                sources.insert(source);
                false
            } else if items.len() >= MAX_ITEMS {
                return Err(zbus::fdo::Error::LimitsExceeded(
                    "Too many tray items".into(),
                ));
            } else {
                items.insert(address.clone(), BTreeSet::from([source]));
                true
            }
        };
        if added {
            self.registered_status_notifier_items_changed(&emitter)
                .await?;
            Self::status_notifier_item_registered(&emitter, &address.registered_name()).await?;
        }
        Ok(())
    }

    async fn register_status_notifier_host(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().ok_or_else(|| invalid("Missing sender"))?;
        resolve(connection, service, Some(sender.as_str())).await?;
        Self::status_notifier_host_registered(&emitter).await?;
        Ok(())
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items
            .lock()
            .expect("tray registry poisoned")
            .keys()
            .map(Address::registered_name)
            .collect()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

pub(super) async fn resolve(
    connection: &Connection,
    service: &str,
    sender: Option<&str>,
) -> zbus::fdo::Result<Address> {
    if service.len() > 4096 {
        return Err(invalid("Tray registration address is too long"));
    }
    let (name, path) = if service.starts_with('/') {
        (
            sender.ok_or_else(|| invalid("Object path registration requires a sender"))?,
            service,
        )
    } else {
        service
            .split_once('/')
            .map_or((service, "/StatusNotifierItem"), |(name, _)| {
                (name, &service[name.len()..])
            })
    };
    zbus::zvariant::ObjectPath::try_from(path).map_err(|_| invalid("Invalid item object path"))?;
    let name =
        zbus::names::BusName::try_from(name).map_err(|_| invalid("Invalid item bus name"))?;
    let owner = zbus::fdo::DBusProxy::new(connection)
        .await?
        .get_name_owner(name)
        .await?;
    Ok(Address {
        owner: owner.to_string(),
        path: path.into(),
    })
}

fn invalid(message: &str) -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs(message.into())
}

pub(super) async fn prune(
    connection: &Connection,
    registry: &Registry,
    live_names: &BTreeSet<String>,
) -> zbus::Result<()> {
    // Preserve the name used for registration. A well-known service can be
    // released/transferred while its process retains the underlying connection.
    // Object-path registrations instead follow the sender's unique name.
    let registrations: Vec<_> = registry
        .lock()
        .expect("tray registry poisoned")
        .iter()
        .flat_map(|(address, sources)| {
            sources
                .iter()
                .map(|source| (address.clone(), source.clone()))
        })
        .collect();
    let dbus = zbus::fdo::DBusProxy::new(connection).await?;
    let invalid = stream::iter(registrations)
        .map(|(address, source)| {
            let dbus = &dbus;
            async move {
                let alive = if !live_names.contains(&source) {
                    false
                } else if source == address.owner {
                    true
                } else {
                    dbus.get_name_owner(
                        source
                            .as_str()
                            .try_into()
                            .expect("validated registration name"),
                    )
                    .await
                    .is_ok_and(|owner| owner.as_str() == address.owner)
                };
                (!alive).then_some((address, source))
            }
        })
        .buffer_unordered(8)
        .filter_map(|value| async move { value })
        .collect::<Vec<_>>()
        .await;
    let removed: Vec<_> = {
        let mut items = registry.lock().expect("tray registry poisoned");
        for (address, source) in invalid {
            if let Some(sources) = items.get_mut(&address) {
                sources.remove(&source);
            }
        }
        let removed = items
            .iter()
            .filter(|(_, sources)| sources.is_empty())
            .map(|(address, _)| address.clone())
            .collect();
        items.retain(|_, sources| !sources.is_empty());
        removed
    };
    if !removed.is_empty() {
        let interface = connection
            .object_server()
            .interface::<_, Watcher>(WATCHER_PATH)
            .await?;
        let emitter = interface.signal_emitter();
        interface
            .get()
            .await
            .registered_status_notifier_items_changed(emitter)
            .await?;
        for address in removed {
            Watcher::status_notifier_item_unregistered(emitter, &address.registered_name()).await?;
        }
    }
    Ok(())
}
