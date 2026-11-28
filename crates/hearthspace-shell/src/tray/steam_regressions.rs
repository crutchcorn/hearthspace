//! Legacy Ayatana peers advertise menus without Activate or ItemIsMenu.
use super::{super::*, support::*};
struct MenuOnly;
#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl MenuOnly {
    #[zbus(property)]
    fn id(&self) -> &str {
        "steam-fixture"
    }
    #[zbus(property)]
    fn menu(&self) -> zbus::zvariant::OwnedObjectPath {
        "/Menu".try_into().unwrap()
    }
}
#[tokio::test]
async fn steam_regressions_menu_only_peer_is_discovered_and_its_menu_can_be_opened() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let connection = bus.connection().await;
    let state = state();
    connection
        .object_server()
        .at("/StatusNotifierItem", MenuOnly)
        .await
        .unwrap();
    connection
        .object_server()
        .at("/Menu", Menu(state.clone()))
        .await
        .unwrap();
    register(&connection, "/StatusNotifierItem").await;
    let snapshot = host.wait(|s| s.items.len() == 1).await;
    let item = &snapshot.items[0];
    assert!(
        item.item_is_menu,
        "a peer with no Activate must open its exported menu"
    );
    host.handle
        .try_send(TrayCommand::OpenMenu {
            key: item.key.clone(),
            menu_path: "/Menu".into(),
            id: 0,
        })
        .unwrap();
    host.wait(|s| {
        s.items[0]
            .menu
            .as_ref()
            .is_some_and(|m| m.items.first().is_some_and(|row| row.label == "Prepared"))
    })
    .await;
    assert!(state.lock().unwrap().prepared);
    host.handle
        .try_send(TrayCommand::MenuClick {
            key: item.key.clone(),
            menu_path: "/Menu".into(),
            id: 1,
        })
        .unwrap();
    wait_action(&state, "event:1:clicked").await;
}
