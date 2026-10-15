//! Transport regressions are built incrementally with the independent mock.
use super::{super::*, support::*};
use zbus::Proxy;

#[tokio::test]
async fn paths_aliases_owner_loss_and_stale_actions_preserve_identity() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let first = state();
    let connection = peer(&bus, first.clone()).await;
    connection
        .request_name("org.hearthspace.TrayTest")
        .await
        .unwrap();
    let second = state();
    second.lock().unwrap().title = "Second item".into();
    connection
        .object_server()
        .at(
            "/OtherItem",
            Item(second.clone(), tokio::runtime::Handle::current()),
        )
        .await
        .unwrap();
    register(&connection, "org.hearthspace.TrayTest").await;
    // An independent path registration survives releasing the named source.
    register(&connection, "/StatusNotifierItem").await;
    register(&connection, "/OtherItem").await;
    register(&connection, "/OtherItem").await;
    let snapshot = host.wait(|snapshot| snapshot.items.len() == 2).await;
    let old_key = snapshot
        .items
        .iter()
        .find(|item| item.title == "Fixture")
        .unwrap()
        .key
        .clone();
    assert_ne!(snapshot.items[0].key.path, snapshot.items[1].key.path);
    assert_eq!(snapshot.items[0].key.owner, snapshot.items[1].key.owner);

    connection
        .release_name("org.hearthspace.TrayTest")
        .await
        .unwrap();
    first.lock().unwrap().status = "NeedsAttention".into();
    signal(
        &connection,
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
        "NewStatus",
    )
    .await;
    let snapshot = host
        .wait(|snapshot| {
            snapshot
                .items
                .iter()
                .any(|item| item.status == TrayStatus::NeedsAttention)
        })
        .await;
    assert_eq!(
        snapshot.items.len(),
        2,
        "releasing an alias is not owner death"
    );
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.key == old_key)
            .unwrap()
            .icon
            .name
            .as_deref(),
        Some("dialog-warning")
    );
    connection.close().await.unwrap();
    host.wait(|snapshot| snapshot.items.is_empty()).await;

    let next = state();
    let connection = peer(&bus, next.clone()).await;
    connection
        .request_name("org.hearthspace.TrayTest")
        .await
        .unwrap();
    register(&connection, "org.hearthspace.TrayTest").await;
    let snapshot = host.wait(|snapshot| snapshot.items.len() == 1).await;
    let key = snapshot.items[0].key.clone();
    assert_ne!(key.generation, old_key.generation);
    host.handle
        .try_send(TrayCommand::Activate {
            key: old_key,
            x: 1,
            y: 2,
        })
        .unwrap();
    host.handle
        .try_send(TrayCommand::Activate {
            key: key.clone(),
            x: 3,
            y: 4,
        })
        .unwrap();
    wait_action(&next, "activate:3:4").await;
    assert!(
        !next
            .lock()
            .unwrap()
            .actions
            .contains(&"activate:1:2".into())
    );
    host.handle
        .try_send(TrayCommand::Secondary {
            key: key.clone(),
            x: 5,
            y: 6,
        })
        .unwrap();
    host.handle
        .try_send(TrayCommand::Context {
            key: key.clone(),
            x: 7,
            y: 8,
        })
        .unwrap();
    host.handle
        .try_send(TrayCommand::Scroll {
            key,
            delta: -120,
            horizontal: true,
        })
        .unwrap();
    wait_action(&next, "secondary:5:6").await;
    wait_action(&next, "context:7:8").await;
    wait_action(&next, "scroll:-120:horizontal").await;
}

#[tokio::test]
async fn menus_prepare_lazy_children_refresh_and_reject_disabled_actions() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let state = state();
    let connection = peer(&bus, state.clone()).await;
    register(&connection, "/StatusNotifierItem").await;
    let snapshot = host.wait(|snapshot| snapshot.items.len() == 1).await;
    let key = snapshot.items[0].key.clone();
    assert!(snapshot.items[0].menu.as_ref().unwrap().items.is_empty());
    host.handle
        .try_send(TrayCommand::OpenMenu {
            menu_path: "/Menu".into(),
            key: key.clone(),
            id: 0,
        })
        .unwrap();
    let snapshot = host
        .wait(|snapshot| {
            snapshot.items[0]
                .menu
                .as_ref()
                .unwrap()
                .items
                .first()
                .is_some_and(|item| item.label == "Prepared")
        })
        .await;
    let submenu = &snapshot.items[0].menu.as_ref().unwrap().items[1];
    assert_eq!(submenu.label, "Sub_menu");
    assert!(submenu.submenu && submenu.children.is_empty());
    host.handle
        .try_send(TrayCommand::OpenMenu {
            menu_path: "/Menu".into(),
            key: key.clone(),
            id: 2,
        })
        .unwrap();
    host.wait(|snapshot| {
        snapshot.items[0].menu.as_ref().unwrap().items[1]
            .children
            .len()
            == 1
    })
    .await;
    host.handle
        .try_send(TrayCommand::MenuClick {
            menu_path: "/Menu".into(),
            key: key.clone(),
            id: 3,
        })
        .unwrap();
    wait_action(&state, "event:3:clicked").await;
    host.handle
        .try_send(TrayCommand::MenuClick {
            menu_path: "/Menu".into(),
            key: key.clone(),
            id: 5,
        })
        .unwrap();
    host.wait(|snapshot| {
        snapshot.items[0].menu.as_ref().unwrap().items[3].toggle == Some(TrayToggle::Check(true))
    })
    .await;
    host.handle
        .try_send(TrayCommand::MenuClick {
            menu_path: "/Menu".into(),
            key,
            id: 4,
        })
        .unwrap();
    host.wait(|snapshot| snapshot.items[0].menu_error.is_some())
        .await;
    assert!(
        !state
            .lock()
            .unwrap()
            .actions
            .contains(&"event:4:clicked".into())
    );
    state.lock().unwrap().checked = false;
    signal(
        &connection,
        "/Menu",
        "com.canonical.dbusmenu",
        "ItemsPropertiesUpdated",
    )
    .await;
    host.wait(|snapshot| {
        snapshot.items[0].menu.as_ref().unwrap().items[3].toggle == Some(TrayToggle::Check(false))
    })
    .await;
}

#[tokio::test]
async fn slow_and_malformed_peers_do_not_block_a_healthy_item() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let slow_state = state();
    slow_state.lock().unwrap().slow = true;
    let slow = peer(&bus, slow_state).await;
    let broken = bus.connection().await;
    broken
        .object_server()
        .at("/Broken", BrokenItem)
        .await
        .unwrap();
    register(&slow, "/StatusNotifierItem").await;
    register(&broken, "/Broken").await;
    let state = state();
    let good = peer(&bus, state.clone()).await;
    register(&good, "/StatusNotifierItem").await;
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        host.wait(|snapshot| snapshot.items.len() == 1),
    )
    .await
    .unwrap();
    state.lock().unwrap().title = "Updated while peer is slow".into();
    signal(
        &good,
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
        "NewTitle",
    )
    .await;
    let snapshot = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        host.wait(|snapshot| {
            snapshot
                .items
                .iter()
                .any(|item| item.title == "Updated while peer is slow")
        }),
    )
    .await
    .unwrap();
    assert!(snapshot.error.is_none());
}

#[tokio::test]
async fn existing_watcher_is_respected_and_worker_cancellation_releases_host_name() {
    let bus = Bus::start();
    let first = Host::start(&bus);
    let connection = peer(&bus, state()).await;
    register(&connection, "/StatusNotifierItem").await;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    let name = watcher::WATCHER_NAME.try_into().unwrap();
    let owner = dbus.get_name_owner(name).await.unwrap();
    let mut second = Host::start(&bus);
    second.wait(|snapshot| snapshot.items.len() == 1).await;
    assert_eq!(
        dbus.get_name_owner(watcher::WATCHER_NAME.try_into().unwrap())
            .await
            .unwrap(),
        owner
    );
    // Abort is the same teardown used by Xilem when removing the owning task.
    drop(first);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if dbus
                .get_name_owner(watcher::WATCHER_NAME.try_into().unwrap())
                .await
                .is_ok_and(|next| next != owner)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    register(&connection, "/StatusNotifierItem").await;
    second.wait(|snapshot| snapshot.items.len() == 1).await;
    let proxy = Proxy::new(
        &connection,
        watcher::WATCHER_NAME,
        watcher::WATCHER_PATH,
        watcher::WATCHER_NAME,
    )
    .await
    .unwrap();
    assert!(
        proxy
            .get_property::<bool>("IsStatusNotifierHostRegistered")
            .await
            .unwrap()
    );
}
