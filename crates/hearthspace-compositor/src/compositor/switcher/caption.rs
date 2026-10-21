//! A session-specific passive shell caption; it can never acquire client focus.
use super::*;
use crate::compositor::windows::window_app_id;
use smithay::utils::{Logical, Point};

impl App {
    pub(in crate::compositor) fn is_window_switcher_caption(&self, index: usize) -> bool {
        window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(hearthspace_common::window_switcher_caption_from_app_id)
            .is_some()
    }
    pub(in crate::compositor) fn window_switcher_caption_visible(&self, index: usize) -> bool {
        let id = window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(hearthspace_common::window_switcher_caption_from_app_id);
        self.window_switcher
            .session
            .as_ref()
            .is_some_and(|s| Some(s.id) == id)
    }
    pub(in crate::compositor) fn window_switcher_caption_rect(&self) -> Option<Rect> {
        let s = self.window_switcher.session.as_ref()?;
        let p = s.caption_preview;
        let width = s.caption_width.unwrap_or(280.).min(p.width).max(1.);
        Some(Rect {
            x: p.x + (p.width - width) / 2.,
            y: p.y + p.height + 10.,
            width,
            height: 24.,
        })
    }
    pub(in crate::compositor) fn window_switcher_caption_origin(
        &self,
        index: usize,
    ) -> Option<Point<i32, Logical>> {
        if !self.window_switcher_caption_visible(index) {
            return None;
        }
        let r = self.window_switcher_caption_rect()?;
        Some(
            Point::from((r.x.round() as i32, r.y.round() as i32))
                - self.window_surface_geometry_loc(index),
        )
    }
    pub(super) fn window_switcher_changed(&mut self) {
        self.window_switcher.revision = self.window_switcher.revision.wrapping_add(1);
        if let Ok(socket) = std::os::unix::net::UnixDatagram::unbound() {
            let _ = socket.set_nonblocking(true);
            let _ = socket.send_to(
                br#"{"version":1,"action":"window-switcher-changed"}"#,
                hearthspace_common::shell_event_socket_path(
                    &super::super::shell_integration::command_socket_path(),
                ),
            );
        }
        self.request_redraw();
    }
}
