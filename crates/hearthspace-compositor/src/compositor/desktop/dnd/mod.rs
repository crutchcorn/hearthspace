//! File drags keep the desktop's existing move gesture and use Smithay offers.
use super::App;
#[cfg(feature = "xwayland")]
use crate::compositor::ManagedSurface;
use crate::compositor::{HitTarget, ManagedWindowKind};
use hearthspace_common::desktop_files::{FileClipboard, local_path};
use smithay::{
    backend::input::InputTime,
    input::dnd::Source,
    utils::{Logical, Point},
};
use std::{collections::BTreeMap, sync::Arc};

mod source;
mod target;
use source::Files;
use target::{Offer, Target};

pub(super) struct Drag {
    source: Arc<Files>,
    target: Option<Target>,
    offer: Option<Offer>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    id: String,
    uri: String,
}

impl App {
    pub(super) fn register_desktop_files(
        &mut self,
        offset: usize,
        entries: Vec<Entry>,
    ) -> Result<(), String> {
        if offset > hearthspace_common::MAX_DESKTOP_ITEMS
            || entries.len() > hearthspace_common::MAX_DESKTOP_ITEMS - offset
            || (offset != 0 && offset != self.desktop.files.len())
        {
            return Err("invalid desktop file registration offset".into());
        }
        let mut files = if offset == 0 {
            BTreeMap::new()
        } else {
            self.desktop.files.clone()
        };
        for entry in entries {
            if !entry.id.starts_with("file:")
                || !self.desktop.order.contains(&entry.id)
                || entry.uri.len() > 16 * 1024
                || files.contains_key(&entry.id)
            {
                return Err("invalid desktop file registration".into());
            }
            local_path(&entry.uri)?;
            files.insert(entry.id, entry.uri);
        }
        self.desktop.files = files;
        Ok(())
    }

    pub(in crate::compositor) fn desktop_file_selection(
        &self,
        ids: &[String],
        cut: bool,
    ) -> Result<FileClipboard, String> {
        let uris = ids
            .iter()
            .map(|id| {
                self.desktop_index(id)
                    .and_then(|_| self.desktop.files.get(id))
                    .cloned()
                    .ok_or_else(|| "A selected desktop file is no longer available.".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let files = FileClipboard {
            cut,
            uris,
            generation: None,
        };
        files.validate()?;
        Ok(files)
    }

    fn desktop_drop_target(&self) -> Option<(Target, Point<f64, Logical>)> {
        let HitTarget::Client {
            window_index,
            surface,
            surface_location,
        } = self.hit_test(self.pointer_location)?
        else {
            return None;
        };
        if self.windows[window_index].kind != ManagedWindowKind::Normal {
            return None;
        }
        let point = self.pointer_location - surface_location;
        #[cfg(feature = "xwayland")]
        if let ManagedSurface::X11(x11) = &self.windows[window_index].surface {
            return Some((Target::X11(x11.clone()), point));
        }
        Some((Target::Wayland(surface), point))
    }

    pub(super) fn update_desktop_drop(&mut self, ids: &[String]) {
        let Some(mut drag) = self.desktop.file_drag.take().or_else(|| {
            self.desktop_file_selection(ids, false)
                .ok()
                .map(|files| Drag {
                    source: Files::new(files),
                    target: None,
                    offer: None,
                })
        }) else {
            return;
        };
        let next = self.desktop_drop_target();
        if drag.target.as_ref() != next.as_ref().map(|(target, _)| target) {
            if let Some(target) = drag.target.take() {
                target.leave(self, drag.offer.as_mut());
            }
            if let Some(offer) = drag.offer.take() {
                offer.disable();
            }
            if let Some((target, point)) = &next {
                drag.offer = target.enter(self, drag.source.clone(), *point);
                drag.target = Some(target.clone());
            }
        } else if let Some((target, point)) = next {
            target.motion(self, drag.offer.as_mut(), point, InputTime::now());
        }
        self.desktop.file_drag = Some(drag);
    }

    /// App drops never relocate the original desktop objects, even if rejected.
    pub(super) fn finish_desktop_drop(&mut self) -> bool {
        let Some(mut drag) = self.desktop.file_drag.take() else {
            return false;
        };
        let on_app = drag.target.is_some();
        let accepted = drag.offer.as_ref().is_some_and(Offer::validated);
        if let Some(target) = drag.target.as_ref().filter(|target| target.alive(self)) {
            target.drop(self, drag.offer.as_mut());
        }
        if accepted {
            if let Some(offer) = &drag.offer {
                offer.dropped();
            }
            drag.source.drop_performed();
        } else {
            if let Some(offer) = &drag.offer {
                offer.disable();
            }
            drag.source.cancel();
        }
        if let Some(target) = drag.target {
            target.leave(self, drag.offer.as_mut());
        }
        on_app
    }

    pub(super) fn cancel_desktop_drop(&mut self) {
        if let Some(mut drag) = self.desktop.file_drag.take() {
            if let Some(target) = drag.target {
                target.leave(self, drag.offer.as_mut());
            }
            if let Some(offer) = drag.offer {
                offer.disable();
            }
            drag.source.cancel();
        }
    }
}
