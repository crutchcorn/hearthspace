use super::{super::*, support::*};
use std::{sync::Arc, time::Duration};

#[tokio::test]
async fn unchanged_named_icon_signal_invalidates_render_cache() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let connection = peer(&bus, state()).await;
    register(&connection, "/StatusNotifierItem").await;
    let initial = host.wait(|snapshot| snapshot.items.len() == 1).await;
    let first_icon = &initial.items[0].icon;
    signal(
        &connection,
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
        "NewIcon",
    )
    .await;
    let updated = host
        .wait(|snapshot| snapshot.items[0].icon.revision != first_icon.revision)
        .await;
    assert_eq!(updated.items[0].icon.name, first_icon.name);
    assert_eq!(updated.items[0].icon.pixmap, first_icon.pixmap);
}

#[tokio::test]
async fn an_old_inflight_layout_cannot_undo_a_newer_menu_action() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let state = state();
    let connection = peer(&bus, state.clone()).await;
    register(&connection, "/StatusNotifierItem").await;
    let snapshot = host.wait(|snapshot| snapshot.items.len() == 1).await;
    let key = snapshot.items[0].key.clone();
    host.handle
        .try_send(TrayCommand::OpenMenu {
            menu_path: "/Menu".into(),
            key: key.clone(),
            id: 0,
        })
        .unwrap();
    host.wait(|snapshot| snapshot.items[0].menu.as_ref().unwrap().items.len() == 4)
        .await;
    let gate = Arc::new(LayoutGate::default());
    state.lock().unwrap().delay_next_layout = Some(gate.clone());
    signal(
        &connection,
        "/Menu",
        "com.canonical.dbusmenu",
        "LayoutUpdated",
    )
    .await;
    tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
        .await
        .unwrap();
    host.handle
        .try_send(TrayCommand::MenuClick {
            menu_path: "/Menu".into(),
            key,
            id: 5,
        })
        .unwrap();
    host.wait(|snapshot| {
        snapshot.items[0].menu.as_ref().unwrap().items[3].toggle == Some(TrayToggle::Check(true))
    })
    .await;
    gate.release.notify_one();
    // If the older response overwrote the action, this is a public change;
    // correct merging keeps the already-published checked menu unchanged.
    assert!(
        tokio::time::timeout(Duration::from_millis(150), host.updates.changed())
            .await
            .is_err()
    );
    assert_eq!(
        host.updates.borrow().items[0].menu.as_ref().unwrap().items[3].toggle,
        Some(TrayToggle::Check(true))
    );
}

#[tokio::test]
async fn a_signal_during_a_read_schedules_one_followup_refresh() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let state = state();
    let connection = peer(&bus, state.clone()).await;
    register(&connection, "/StatusNotifierItem").await;
    let snapshot = host.wait(|snapshot| snapshot.items.len() == 1).await;
    host.handle
        .try_send(TrayCommand::OpenMenu {
            menu_path: "/Menu".into(),
            key: snapshot.items[0].key.clone(),
            id: 0,
        })
        .unwrap();
    host.wait(|snapshot| snapshot.items[0].menu.as_ref().unwrap().items.len() == 4)
        .await;
    let gate = Arc::new(LayoutGate::default());
    state.lock().unwrap().delay_next_layout = Some(gate.clone());
    signal(
        &connection,
        "/Menu",
        "com.canonical.dbusmenu",
        "LayoutUpdated",
    )
    .await;
    tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
        .await
        .unwrap();
    state.lock().unwrap().checked = true;
    signal(
        &connection,
        "/Menu",
        "com.canonical.dbusmenu",
        "ItemsPropertiesUpdated",
    )
    .await;
    gate.release.notify_one();
    tokio::time::timeout(
        Duration::from_secs(1),
        host.wait(|snapshot| {
            snapshot.items[0].menu.as_ref().unwrap().items[3].toggle
                == Some(TrayToggle::Check(true))
        }),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn replacing_a_menu_path_rejects_old_actions_before_calling_either_menu() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let state = state();
    let connection = peer(&bus, state.clone()).await;
    connection
        .object_server()
        .at("/OtherMenu", Menu(state.clone()))
        .await
        .unwrap();
    register(&connection, "/StatusNotifierItem").await;
    let snapshot = host.wait(|snapshot| snapshot.items.len() == 1).await;
    let key = snapshot.items[0].key.clone();
    host.handle
        .try_send(TrayCommand::OpenMenu {
            key: key.clone(),
            menu_path: "/Menu".into(),
            id: 0,
        })
        .unwrap();
    host.wait(|snapshot| snapshot.items[0].menu.as_ref().unwrap().items.len() == 4)
        .await;
    let before = state.lock().unwrap().actions.clone();
    // Deliberately omit the change signal: the transport must verify the
    // fresh Menu property, not trust either its own or zbus's property cache.
    state.lock().unwrap().menu_path = "/OtherMenu".into();
    host.handle
        .try_send(TrayCommand::MenuClick {
            key: key.clone(),
            menu_path: "/Menu".into(),
            id: 5,
        })
        .unwrap();
    host.wait(|snapshot| snapshot.items[0].menu_error.is_some())
        .await;
    assert_eq!(state.lock().unwrap().actions, before);
    signal(
        &connection,
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
        "NewIcon",
    )
    .await;
    host.wait(|snapshot| snapshot.items[0].menu_path.as_deref() == Some("/OtherMenu"))
        .await;
    host.handle
        .try_send(TrayCommand::OpenMenu {
            key: key.clone(),
            menu_path: "/Menu".into(),
            id: 0,
        })
        .unwrap();
    host.wait(|snapshot| snapshot.items[0].menu_error.is_some())
        .await;
    assert_eq!(state.lock().unwrap().actions, before);
    host.handle
        .try_send(TrayCommand::OpenMenu {
            key,
            menu_path: "/OtherMenu".into(),
            id: 0,
        })
        .unwrap();
    host.wait(|snapshot| {
        snapshot.items[0].menu.as_ref().unwrap().items.len() == 4
            && snapshot.items[0].menu_error.is_none()
    })
    .await;
}

#[tokio::test]
async fn losing_the_registered_alias_removes_item_while_connection_stays_alive() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let connection = peer(&bus, state()).await;
    connection
        .request_name("org.hearthspace.NamedTray")
        .await
        .unwrap();
    register(&connection, "org.hearthspace.NamedTray").await;
    let first = host.wait(|snapshot| snapshot.items.len() == 1).await;
    connection
        .release_name("org.hearthspace.NamedTray")
        .await
        .unwrap();
    host.wait(|snapshot| snapshot.items.is_empty()).await;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    assert!(
        dbus.name_has_owner(
            connection
                .unique_name()
                .unwrap()
                .as_str()
                .try_into()
                .unwrap()
        )
        .await
        .unwrap()
    );
    connection
        .request_name("org.hearthspace.NamedTray")
        .await
        .unwrap();
    register(&connection, "org.hearthspace.NamedTray").await;
    let second = host.wait(|snapshot| snapshot.items.len() == 1).await;
    assert_eq!(first.items[0].key.owner, second.items[0].key.owner);
    assert_ne!(
        first.items[0].key.generation,
        second.items[0].key.generation
    );
}
