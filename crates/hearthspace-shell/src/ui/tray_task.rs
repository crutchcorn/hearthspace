//! One session-scoped worker; decoding icons never blocks the UI thread.
use super::{state::ShellState, tray_state};
use crate::tray::{ItemKey, TrayIcon, TrayPixmap, TraySnapshot};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

pub(super) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<TraySnapshot>, state: &mut ShellState| {
            let address = std::env::var("HEARTHSPACE_SESSION_BUS_ADDRESS").ok();
            let (handle, mut snapshots, worker) = crate::tray::channel(address);
            state.tray.handle = Some(handle);
            async move {
                let render = async move {
                    let mut cache = Vec::<(ItemKey, TrayIcon, Option<TrayPixmap>)>::new();
                    loop {
                        let snapshot = snapshots.borrow_and_update().clone();
                        let resolved = xilem::tokio::task::spawn_blocking(move || {
                            let mut snapshot = snapshot;
                            for item in &mut snapshot.items {
                                let icon = item.icon.clone();
                                let pixmap = if let Some((_, _, pixmap)) = cache
                                    .iter()
                                    .find(|(owner, key, _)| owner == &item.key && key == &icon)
                                {
                                    pixmap.clone()
                                } else {
                                    let value = super::tray_icon_loader::resolve(&icon);
                                    if cache.len() >= 128 {
                                        cache.remove(0);
                                    }
                                    cache.push((item.key.clone(), icon, value.clone()));
                                    value
                                };
                                item.icon.pixmap = pixmap;
                            }
                            (snapshot, cache)
                        })
                        .await;
                        let Ok((snapshot, next_cache)) = resolved else {
                            break;
                        };
                        cache = next_cache;
                        if proxy.message(snapshot).is_err() || snapshots.changed().await.is_err() {
                            break;
                        }
                    }
                };
                xilem::tokio::select! { _ = worker.run() => {}, _ = render => {} }
            }
        },
        |state: &mut ShellState, snapshot| {
            state.tray.snapshot = snapshot;
            tray_state::reconcile(state);
        },
    )
}
