use futures_util::{StreamExt, stream};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinSet,
};
use zbus::{
    Connection, MatchRule, MessageStream, Proxy,
    connection::Builder,
    fdo::{DBusProxy, RequestNameFlags, RequestNameReply},
    message::Type,
};

use super::{
    ItemKey, TrayCommand, TrayMenu, TraySnapshot, actions, menu,
    properties::{self, Item, error},
    watcher::{self, Address, MAX_ITEMS, Registry, WATCHER_NAME, WATCHER_PATH, Watcher},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
type ItemResult = (ItemKey, Result<Item, String>);
type ActionResult = (ItemKey, Option<String>, Result<Option<TrayMenu>, String>);

pub(super) struct Session {
    connection: Connection,
    registry: Option<Registry>,
    watcher_owner: String,
    signals: MessageStream,
    discovery: JoinSet<Result<Vec<Address>, String>>,
    reads: JoinSet<ItemResult>,
    actions: JoinSet<ActionResult>,
}

impl Session {
    pub async fn connect(address: &str) -> Result<Self, String> {
        let connection = Builder::address(address)
            .map_err(error)?
            .method_timeout(Duration::from_millis(750))
            .max_queued(32)
            .build()
            .await
            .map_err(error)?;
        let registry = Arc::new(Mutex::new(BTreeMap::new()));
        let rule = MatchRule::builder().msg_type(Type::Signal).build();
        let signals = MessageStream::for_match_rule(rule, &connection, Some(64))
            .await
            .map_err(error)?;
        connection
            .object_server()
            .at(
                WATCHER_PATH,
                Watcher {
                    items: registry.clone(),
                },
            )
            .await
            .map_err(error)?;
        let flags = RequestNameFlags::DoNotQueue.into();
        let owns_watcher = match connection
            .request_name_with_flags(WATCHER_NAME, flags)
            .await
        {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => true,
            Ok(_) | Err(zbus::Error::NameTaken) => false,
            Err(err) => return Err(error(err)),
        };
        // Export before acquiring the name, then register as a host. Existing
        // watchers keep ownership; do not queue or replace their bus name.
        let suffix = connection
            .unique_name()
            .expect("bus connection has a unique name")
            .as_str()
            .replace([':', '.'], "_");
        let name = format!("org.kde.StatusNotifierHost.hearthspace.c{suffix}");
        connection.request_name(name.clone()).await.map_err(error)?;
        let proxy = Proxy::new(&connection, WATCHER_NAME, WATCHER_PATH, WATCHER_NAME)
            .await
            .map_err(error)?;
        proxy
            .call::<_, _, ()>("RegisterStatusNotifierHost", &(name,))
            .await
            .map_err(error)?;
        let watcher_owner = DBusProxy::new(&connection)
            .await
            .map_err(error)?
            .get_name_owner(WATCHER_NAME.try_into().expect("valid watcher name"))
            .await
            .map_err(error)?
            .to_string();
        Ok(Self {
            connection,
            registry: owns_watcher.then_some(registry),
            signals,
            watcher_owner,
            discovery: JoinSet::new(),
            reads: JoinSet::new(),
            actions: JoinSet::new(),
        })
    }

    pub async fn shutdown(mut self) {
        self.discovery.shutdown().await;
        self.reads.shutdown().await;
        self.actions.shutdown().await;
        let _ = self.connection.close().await;
    }

    pub async fn run(
        &mut self,
        commands: &mut mpsc::Receiver<TrayCommand>,
        updates: &watch::Sender<TraySnapshot>,
        generation: &mut u64,
    ) -> Result<(), String> {
        let mut known = BTreeMap::<Address, ItemKey>::new();
        let mut items = BTreeMap::<ItemKey, Item>::new();
        let mut queued = VecDeque::<ItemKey>::new();
        let mut pending = BTreeSet::<ItemKey>::new();
        let mut dirty = BTreeSet::<ItemKey>::new();
        let mut polling = tokio::time::interval(Duration::from_secs(10));
        polling.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut flushing = tokio::time::interval(Duration::from_millis(50));
        flushing.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut changed = true;
        let mut discover_needed = true;
        loop {
            if discover_needed && self.discovery.is_empty() {
                let connection = self.connection.clone();
                let registry = self.registry.clone();
                let watcher_owner = self.watcher_owner.clone();
                self.discovery.spawn(async move {
                    discover(&connection, registry.as_ref(), &watcher_owner).await
                });
                discover_needed = false;
            }
            while self.reads.len() < 8 {
                let Some(key) = queued.pop_front() else {
                    break;
                };
                if !known.values().any(|current| *current == key) {
                    pending.remove(&key);
                    continue;
                }
                let old = items.get(&key).cloned();
                let connection = self.connection.clone();
                self.reads.spawn(async move {
                    let result = tokio::time::timeout(
                        REQUEST_TIMEOUT,
                        refresh(&connection, key.clone(), old),
                    )
                    .await
                    .unwrap_or_else(|_| Err("Tray application timed out".into()));
                    (key, result)
                });
            }
            tokio::select! {
                _ = self.connection.closed() => return Err("Tray session bus disconnected".into()),
                _ = updates.closed() => return Ok(()),
                _ = polling.tick() => discover_needed = true,
                Some(signal) = self.signals.next() => match signal {
                    Ok(message) => {
                        let header = message.header();
                        let interface = header.interface().map(|name| name.as_str()).unwrap_or_default();
                        let member = header.member().map(|name| name.as_str()).unwrap_or_default();
                        if interface == "org.freedesktop.DBus" && member == "NameOwnerChanged" {
                            if let Ok((name, old, new)) = message.body().deserialize::<(String, String, String)>() {
                                let registered_source = self.registry.as_ref().is_some_and(|registry| registry.lock().expect("tray registry poisoned").values().any(|sources| sources.contains(&name)));
                                discover_needed |= name == WATCHER_NAME || (!old.is_empty() && old != new && (registered_source || known.values().any(|key| key.owner == name)));
                            }
                        } else if interface == WATCHER_NAME && header.sender().is_some_and(|sender| sender.as_str() == self.watcher_owner) {
                            discover_needed = true;
                        } else if matches!(interface, "org.kde.StatusNotifierItem" | "com.canonical.dbusmenu" | "org.freedesktop.DBus.Properties") {
                            for key in known.values() {
                                let own_path = header.path().is_some_and(|path| path.as_str() == key.path);
                                let menu_path = items.get(key).and_then(|item| item.menu_path.as_deref());
                                let own_menu = header.path().is_some_and(|path| Some(path.as_str()) == menu_path);
                                if header.sender().is_some_and(|name| name.as_str() == key.owner) && (own_path || own_menu) {
                                    if own_path && matches!(member, "NewIcon" | "NewAttentionIcon" | "NewOverlayIcon" | "PropertiesChanged")
                                        && let Some(item) = items.get_mut(key) {
                                        item.view.icon.revision = item.view.icon.revision.wrapping_add(1);
                                        changed = true;
                                    }
                                    if pending.insert(key.clone()) { queued.push_front(key.clone()); }
                                    else { dirty.insert(key.clone()); }
                                }
                            }
                        }
                    }
                    Err(_) => discover_needed = true,
                },
                Some(result) = self.discovery.join_next() => {
                    let addresses = result.map_err(error)??;
                    known.retain(|address, _| addresses.contains(address));
                    let retained: BTreeSet<_> = known.values().cloned().collect();
                    dirty.retain(|key| retained.contains(key));
                    let before = items.len();
                    items.retain(|key, _| retained.contains(key));
                    changed |= before != items.len();
                    for address in addresses {
                        let key = known.entry(address.clone()).or_insert_with(|| {
                            *generation = generation.wrapping_add(1);
                            ItemKey { owner: address.owner, path: address.path, generation: *generation }
                        }).clone();
                        if pending.insert(key.clone()) {
                            // New registrations are read before existing icons.
                            if items.contains_key(&key) { queued.push_back(key); }
                            else { queued.push_front(key); }
                        }
                    }
                }
                Some(result) = self.reads.join_next() => {
                    let (key, result) = result.map_err(error)?;
                    pending.remove(&key);
                    if !known.values().any(|current| *current == key) { continue; }
                    if dirty.remove(&key) {
                        pending.insert(key.clone());
                        queued.push_back(key.clone());
                    }
                    match result {
                        Ok(mut item) => {
                            if let Some(old) = items.get(&key) { item.view.icon.revision = old.view.icon.revision; }
                            // A menu opened after this property fetch began is
                            // newer than the empty menu placeholder we fetched.
                            if let Some(old) = items.get(&key)
                                && old.menu_path == item.menu_path &&
                                    (old.menu_revision != item.menu_revision || old.menu_loaded && !item.menu_loaded) {
                                    item.view.menu = old.view.menu.clone();
                                    item.view.menu_error = old.view.menu_error.clone();
                                    item.menu_loaded = old.menu_loaded;
                                    item.menu_revision = old.menu_revision;
                            }
                            changed |= items.get(&key).is_none_or(|old| old.view != item.view);
                            items.insert(key, item);
                        }
                        Err(error) => {
                            tracing::debug!(?key, %error, "tray item unavailable");
                            changed |= items.remove(&key).is_some();
                        }
                    }
                }
                command = commands.recv(), if self.actions.is_empty() => {
                    let Some(command) = command else { return Ok(()); };
                    let key = command.key().clone();
                    if let Some(item) = items.get_mut(&key) {
                        item.menu_revision = item.menu_revision.wrapping_add(1);
                        let item = item.clone();
                        let connection = self.connection.clone();
                        self.actions.spawn(async move {
                            let result = tokio::time::timeout(REQUEST_TIMEOUT, actions::perform(&connection, &item, command)).await
                                .unwrap_or_else(|_| Err("Tray action timed out".into()));
                            (key, item.menu_path, result)
                        });
                    }
                }
                Some(result) = self.actions.join_next() => {
                    let (key, menu_path, result) = result.map_err(error)?;
                    if let Some(item) = items.get_mut(&key) {
                        item.menu_revision = item.menu_revision.wrapping_add(1);
                        if item.menu_path != menu_path { continue; }
                        match result {
                            Ok(Some(menu)) => {
                                item.view.menu = Some(menu);
                                item.menu_loaded = true;
                                item.view.menu_error = None;
                            }
                            Ok(None) => item.view.menu_error = None,
                            Err(error) => item.view.menu_error = Some(error),
                        }
                        changed = true;
                    }
                }
                _ = flushing.tick(), if changed => {
                    let snapshot = TraySnapshot { items: items.values().map(|item| item.view.clone()).collect(), error: None };
                    updates.send_if_modified(|current| {
                        if *current == snapshot { false } else { *current = snapshot; true }
                    });
                    changed = false;
                }
            }
        }
    }
}

async fn discover(
    connection: &Connection,
    registry: Option<&Registry>,
    watcher_owner: &str,
) -> Result<Vec<Address>, String> {
    let dbus = DBusProxy::new(connection).await.map_err(error)?;
    let owner = dbus
        .get_name_owner(WATCHER_NAME.try_into().expect("valid watcher name"))
        .await
        .map_err(error)?;
    if owner.as_str() != watcher_owner {
        return Err("Tray watcher restarted".into());
    }
    let names: BTreeSet<_> = dbus
        .list_names()
        .await
        .map_err(error)?
        .into_iter()
        .map(|name| name.to_string())
        .collect();
    if let Some(registry) = registry {
        watcher::prune(connection, registry, &names)
            .await
            .map_err(error)?;
        return Ok(registry
            .lock()
            .expect("tray registry poisoned")
            .keys()
            .cloned()
            .collect());
    }
    let proxy = Proxy::new(connection, WATCHER_NAME, WATCHER_PATH, WATCHER_NAME)
        .await
        .map_err(error)?;
    let services: Vec<String> = proxy
        .get_property("RegisteredStatusNotifierItems")
        .await
        .map_err(error)?;
    let mut addresses = stream::iter(services.into_iter().take(MAX_ITEMS))
        .map(|service| async move { watcher::resolve(connection, &service, None).await.ok() })
        .buffer_unordered(8)
        .filter_map(|address| async move { address })
        .collect::<Vec<_>>()
        .await;
    addresses.sort();
    addresses.dedup();
    Ok(addresses)
}

async fn refresh(connection: &Connection, key: ItemKey, old: Option<Item>) -> Result<Item, String> {
    let mut item = properties::read(connection, key).await?;
    item.menu_revision = old.as_ref().map_or(0, |old| old.menu_revision);
    if let Some(old) = old.filter(|old| old.menu_loaded && old.menu_path == item.menu_path) {
        let path = item.menu_path.as_deref().expect("loaded menu has a path");
        match menu::load(connection, &item.view.key.owner, path, None).await {
            Ok(menu) => item.view.menu = Some(menu),
            Err(error) => {
                item.view.menu = old.view.menu;
                item.view.menu_error = Some(error);
            }
        }
        item.menu_loaded = true;
    }
    Ok(item)
}
