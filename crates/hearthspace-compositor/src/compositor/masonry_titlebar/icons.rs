//! Resolve application artwork away from the compositor's input/render thread.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, mpsc},
};

use calloop::channel;
use hearthspace_desktop::{app_catalog::AppCatalog, artwork::RasterImage};

use crate::compositor::App;

const MAX_ICONS: usize = 128;
const MAX_PENDING: usize = 32;
type Artwork = Option<Arc<RasterImage>>;

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
                    let icon =
                        hearthspace_desktop::app_icon::resolve_window(&catalog, &id).map(Arc::new);
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
