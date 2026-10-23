//! The original live SNI menu remains usable alongside a matching pinned dock app.
use super::*;
use crate::tray::tests::support::{Bus, Host, peer, register, signal, wait_action};

#[tokio::test]
async fn live_tray_beside_running_pinned_app_keeps_original_dbusmenu_and_context_actions() {
    let bus = Bus::start();
    let mut host = Host::start(&bus);
    let peer_state = crate::tray::tests::support::state();
    let connection = peer(&bus, peer_state.clone()).await;
    register(&connection, "/StatusNotifierItem").await;
    let snapshot = host.wait(|snapshot| snapshot.items.len() == 1).await;
    let handle = host.handle.clone();
    let mut updates = host.updates.clone();
    let runtime = tokio::runtime::Handle::current();
    let peer_connection = connection.clone();
    tokio::task::spawn_blocking(move || {
        let dir = tempfile::tempdir().unwrap();
        let (mut state, _worker) = fixture(dir.path());
        install(dir.path(), "fixture", "Icon=steam\n");
        state.catalog = AppCatalog::load_from_data_dirs(vec![dir.path().to_owned()]);
        state.windows[0].app_id = "fixture".into();
        state.favorites.store.add("fixture.desktop").unwrap();
        state.tray.snapshot = snapshot;
        state.tray.handle = Some(handle);
        let mut ui = Ui::new(state, view, (500, 300));
        assert_eq!(tray_state::visible(&ui.state.tray.snapshot).count(), 1);
        assert_ne!(ui.button("Fixture"), ui.button("Switch to Steam, pinned"));
        ui.harness
            .mouse_click_on(ui.button("Fixture"), Some(PointerButton::Secondary));
        ui.dispatch(&view);
        assert!(ui.state.outputs[0].tray.open);
        ui.state.tray.snapshot =
            runtime.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    loop {
                        let snapshot = updates.borrow_and_update().clone();
                        if snapshot.items[0].menu.as_ref().is_some_and(|menu| {
                            menu.items.iter().any(|item| item.label == "Prepared")
                        }) {
                            break snapshot;
                        }
                        updates.changed().await.unwrap();
                    }
                })
                .await
                .unwrap()
            });
        tray_state::reconcile(&mut ui.state);
        ui.rebuild(&view);
        ui.dispatch(&view);
        let action = ids(&ui.harness)
            .into_iter()
            .find(|id| {
                ui.harness.access_node(*id).is_some_and(|node| {
                    node.role() == Role::MenuItem && node.label().as_deref() == Some("Prepared")
                })
            })
            .unwrap();
        ui.activate(action);
        ui.dispatch(&view);
        runtime.block_on(wait_action(&peer_state, "event:1:clicked"));
        assert!(!ui.state.outputs[0].tray.open);

        // Removing the exported menu keeps the same SNI icon and its legacy
        // ContextMenu route while the dock app remains present.
        peer_state.lock().unwrap().menu_path = "/".into();
        ui.state.tray.snapshot = runtime.block_on(async {
            signal(
                &peer_connection,
                "/StatusNotifierItem",
                "org.kde.StatusNotifierItem",
                "NewIcon",
            )
            .await;
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let snapshot = updates.borrow_and_update().clone();
                    if snapshot.items[0].menu_path.is_none() {
                        break snapshot;
                    }
                    updates.changed().await.unwrap();
                }
            })
            .await
            .unwrap()
        });
        ui.rebuild(&view);
        ui.dispatch(&view);
        assert_eq!(tray_state::visible(&ui.state.tray.snapshot).count(), 1);
        ui.harness
            .mouse_click_on(ui.button("Fixture"), Some(PointerButton::Secondary));
        ui.dispatch(&view);
        runtime.block_on(wait_action(&peer_state, "context:0:0"));
        assert!(
            !peer_state
                .lock()
                .unwrap()
                .actions
                .iter()
                .any(|action| action.starts_with("activate:"))
        );
    })
    .await
    .unwrap();
}
