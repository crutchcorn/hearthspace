//! Standard foreign handles describe one mapping, never an application or stack index.
use std::collections::HashMap;

use smithay::{
    utils::{Buffer, Logical, Rectangle, Scale, Size},
    wayland::{
        foreign_toplevel_list::{
            ForeignToplevelHandle, ForeignToplevelListHandler, ForeignToplevelListState,
            ForeignToplevelWeakHandle,
        },
        image_capture_source::{ToplevelCaptureSourceHandler, ToplevelCaptureSourceState},
    },
};

use super::{
    App, DisplayHandle, ExtForeignToplevelListV1, ForeignToplevelListGlobalData, GlobalDispatch,
    ImageCaptureSource, RemoteCaptureState,
};
use crate::compositor::{ManagedSurface, windows};

pub(super) struct Toplevels {
    pub list: ForeignToplevelListState,
    pub source: ToplevelCaptureSourceState,
    handles: HashMap<u64, ForeignToplevelHandle>,
}

impl Toplevels {
    pub fn new<D>(display: &DisplayHandle, enabled: bool) -> Self
    where
        D: ForeignToplevelListHandler
            + ToplevelCaptureSourceHandler
            + GlobalDispatch<ExtForeignToplevelListV1, ForeignToplevelListGlobalData>,
    {
        Self {
            list: ForeignToplevelListState::new_with_filter::<D>(display, move |_| enabled),
            source: ToplevelCaptureSourceState::new_with_filter::<D, _>(display, move |_| enabled),
            handles: HashMap::new(),
        }
    }
}

struct WindowId(u64);
struct Source(ForeignToplevelWeakHandle);

pub(super) fn source_window_id(source: &ImageCaptureSource) -> Option<u64> {
    let handle = source.user_data().get::<Source>()?.0.upgrade()?;
    Some(handle.user_data().get::<WindowId>()?.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WindowGeometry {
    pub rect: Rectangle<i32, Logical>,
    pub scale: Scale<f64>,
    pub size: Size<i32, Buffer>,
}

/// Choosers consume one title per line. This is display metadata only.
fn display_title(title: &str) -> String {
    title
        .chars()
        .take(512)
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}

impl RemoteCaptureState {
    pub(in crate::compositor) fn retire_toplevel(&mut self, id: u64) {
        self.sessions
            .retain(|s| source_window_id(&s.source()) != Some(id));
        self.window_requests
            .retain(|r| source_window_id(&r.session.source()) != Some(id));
        if let Some(handle) = self.toplevels.handles.remove(&id) {
            // Protocol objects can retain strong references: dropping our handle
            // alone cannot close it. A remap receives a fresh opaque identifier.
            self.toplevels.list.remove_toplevel(&handle);
        }
    }
}

impl App {
    pub(in crate::compositor) fn sync_capture_toplevel(&mut self, id: u64) {
        if !self.remote_capture.enabled {
            return;
        }
        let Some(index) = self
            .windows
            .index(id)
            .filter(|i| self.is_navigable_window(*i))
        else {
            self.remote_capture.retire_toplevel(id);
            return;
        };
        let window = &self.windows[index];
        let title = display_title(&windows::window_title(&window.surface).unwrap_or_default());
        let app_id = windows::window_app_id(&window.surface).unwrap_or_default();
        let toplevels = &mut self.remote_capture.toplevels;
        if let Some(handle) = toplevels.handles.get(&id) {
            if handle.title() != title || handle.app_id() != app_id {
                handle.send_title(&title);
                handle.send_app_id(&app_id);
                handle.send_done();
            }
        } else {
            let handle = toplevels.list.new_toplevel::<Self>(title, app_id);
            handle.user_data().insert_if_missing(|| WindowId(id));
            toplevels.handles.insert(id, handle);
        }
    }

    pub(super) fn capture_window_geometry(
        &self,
        source: &ImageCaptureSource,
    ) -> Option<(usize, WindowGeometry)> {
        let handle = source.user_data().get::<Source>()?.0.upgrade()?;
        if handle.is_closed() {
            return None;
        }
        let id = handle.user_data().get::<WindowId>()?.0;
        if !self
            .remote_capture
            .toplevels
            .handles
            .get(&id)?
            .matches(&handle)
        {
            return None;
        }
        let index = self.windows.index(id)?;
        if !self.is_navigable_window(index) {
            return None;
        }
        let root = self.windows[index].surface.wl_surface()?;
        let rect = match &self.windows[index].surface {
            ManagedSurface::Xdg(_) => super::window_tree::effective_geometry(&root),
            #[cfg(feature = "xwayland")]
            // Smithay's bbox is surface-local (Rectangle::from_size), and
            // geometry subtracts CSD frame extents. X root position belongs
            // to last_configure(), which must never offset this capture.
            ManagedSurface::X11(surface) => surface.geometry(),
        };
        let scale =
            super::window_tree::backing_scale(&self.capture_window_trees(index, rect.loc), rect);
        let size = Size::from((
            (f64::from(rect.size.w) * scale.x).ceil() as i32,
            (f64::from(rect.size.h) * scale.y).ceil() as i32,
        ));
        super::constraints(size)?;
        Some((index, WindowGeometry { rect, scale, size }))
    }
}

impl ForeignToplevelListHandler for App {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.remote_capture.toplevels.list
    }
}

impl ToplevelCaptureSourceHandler for App {
    fn toplevel_capture_source_state(&mut self) -> &mut ToplevelCaptureSourceState {
        &mut self.remote_capture.toplevels.source
    }

    fn toplevel_source_created(
        &mut self,
        source: ImageCaptureSource,
        toplevel: ForeignToplevelHandle,
    ) {
        // Smithay resolves protocol resources, but closed handles still resolve.
        if !toplevel.is_closed() && toplevel.user_data().get::<WindowId>().is_some() {
            source
                .user_data()
                .insert_if_missing(|| Source(toplevel.downgrade()));
        }
    }
}
