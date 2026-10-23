//! Resolve application artwork away from the compositor's input/render thread.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, mpsc},
};

use calloop::channel;
use hearthspace_shell::{
    app_catalog::AppCatalog,
    tray::{TrayIcon, TrayPixmap},
};

use crate::compositor::App;

const MAX_ICONS: usize = 128;
const MAX_PENDING: usize = 32;
type Artwork = Option<Arc<TrayPixmap>>;

#[derive(Default)]
pub(in crate::compositor) struct State {
    cache: BTreeMap<String, Artwork>,
    order: Vec<String>,
    pending: BTreeSet<String>,
    requests: Option<mpsc::SyncSender<String>>,
}

impl App {
    pub(in crate::compositor) fn titlebar_app_icon(&mut self, app_id: Option<&str>) -> Artwork {
        let id = app_id.filter(|id| !id.is_empty() && id.len() <= 1024)?;
        // These native apps intentionally use the website's matching vector glyphs.
        if super::widget::native_app_icon(id).is_some() {
            return None;
        }
        if let Some(icon) = self.titlebar_icons.cache.get(id) {
            return icon.clone();
        }
        if self.titlebar_icons.pending.contains(id)
            || self.titlebar_icons.pending.len() >= MAX_PENDING
        {
            return None;
        }
        if self.titlebar_icons.requests.is_none() && !self.start_titlebar_icon_worker() {
            return None;
        }
        if self
            .titlebar_icons
            .requests
            .as_ref()
            .is_some_and(|sender| sender.try_send(id.to_owned()).is_ok())
        {
            self.titlebar_icons.pending.insert(id.to_owned());
        }
        None
    }

    fn start_titlebar_icon_worker(&mut self) -> bool {
        let (requests, receive) = mpsc::sync_channel::<String>(MAX_PENDING);
        let (complete, results) = channel::channel::<(String, Artwork)>();
        let Ok(token) = self.loop_handle.insert_source(results, |event, _, data| {
            if let channel::Event::Msg((id, icon)) = event {
                let state = &mut data.state.titlebar_icons;
                state.pending.remove(&id);
                if state.cache.len() >= MAX_ICONS && !state.order.is_empty() {
                    state.cache.remove(&state.order.remove(0));
                }
                state.order.push(id.clone());
                state.cache.insert(id, icon);
                data.state.request_redraw();
            }
        }) else {
            return false;
        };
        if std::thread::Builder::new()
            .name("titlebar-icons".into())
            .spawn(move || {
                let catalog = AppCatalog::load();
                while let Ok(id) = receive.recv() {
                    let name = icon_name(&catalog, &id);
                    let icon = hearthspace_shell::app_icon::resolve(&TrayIcon {
                        name: Some(name),
                        ..Default::default()
                    })
                    .map(Arc::new);
                    if complete.send((id, icon)).is_err() {
                        break;
                    }
                }
            })
            .is_err()
        {
            self.loop_handle.remove(token);
            return false;
        }
        self.titlebar_icons.requests = Some(requests);
        true
    }
}

fn icon_name(catalog: &AppCatalog, app_id: &str) -> String {
    let id = app_id.strip_suffix(".desktop").unwrap_or(app_id);
    catalog
        .app_for_window(app_id)
        .and_then(|app| app.icon.clone())
        .unwrap_or_else(|| id.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_identity_with_a_desktop_suffix_keeps_its_declared_artwork() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("applications")).unwrap();
        std::fs::write(
            dir.path().join("applications/ai.opencode.desktop.desktop"),
            "[Desktop Entry]\nType=Application\nName=OpenCode\nExec=/opt/OpenCode/ai.opencode.desktop %U\nIcon=ai.opencode.desktop\nStartupWMClass=ai.opencode.desktop\n",
        )
        .unwrap();
        let catalog = AppCatalog::load_from_data_dirs(vec![dir.path().to_owned()]);

        assert_eq!(
            icon_name(&catalog, "ai.opencode.desktop"),
            "ai.opencode.desktop"
        );
        assert_eq!(
            icon_name(&catalog, "ai.opencode.desktop.desktop"),
            "ai.opencode.desktop"
        );
    }

    #[test]
    fn x11_class_resolves_the_desktop_entry_artwork_not_the_class_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("applications")).unwrap();
        std::fs::write(
            dir.path().join("applications/vlc_vlc.desktop"),
            "[Desktop Entry]\nType=Application\nName=VLC media player\nExec=/snap/bin/vlc --started-from-file %U\nIcon=/snap/vlc/current/vlc.png\nX-SnapInstanceName=vlc\nX-SnapAppName=vlc\nX-SnapCommonID=org.videolan.vlc\n",
        )
        .unwrap();
        let catalog = AppCatalog::load_from_data_dirs(vec![dir.path().to_owned()]);
        assert_eq!(icon_name(&catalog, "Vlc"), "/snap/vlc/current/vlc.png");
        assert_eq!(
            icon_name(&catalog, "vlc_vlc.desktop"),
            "/snap/vlc/current/vlc.png"
        );
        assert_eq!(
            icon_name(&catalog, "org.videolan.vlc"),
            "/snap/vlc/current/vlc.png"
        );
        assert_eq!(icon_name(&catalog, "Unknown"), "unknown");
    }
}
