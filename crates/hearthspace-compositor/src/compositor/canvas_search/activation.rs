//! Result activation keeps add-widget actions in the placement owner.
use super::*;

impl App {
    pub(super) fn activate_canvas_search(&mut self, id: Option<String>) -> Result<(), String> {
        let s = self
            .canvas_search
            .session
            .as_ref()
            .ok_or("canvas search is inactive")?;
        let id = id
            .or_else(|| s.selected.clone())
            .ok_or("no search result selected")?;
        let result = s
            .results
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or("search result is unavailable")?;
        let output = s.output.clone();
        let view = s.view;
        if result.kind == "widget" {
            let kind = result
                .id
                .strip_prefix("widget:")
                .ok_or("invalid widget result")?;
            let output = output.as_ref().ok_or("widget output disappeared")?;
            let placement = hearthspace_common::frame_layout::ObjectPlacement::Canvas {
                x: Some(s.add_at.x),
                y: Some(s.add_at.y),
                width: None,
                height: None,
            };
            self.layout_editor
                .layouts
                .get_mut(output)
                .ok_or("widget output disappeared")?
                .add_widget(kind, placement)?;
            self.close_canvas_search(true);
            self.reconcile_layout_editor();
            self.layout_changed();
            return Ok(());
        }
        self.close_canvas_search(false);
        if result.kind == "window" {
            let id = result.window.ok_or("window is unavailable")?;
            let index = self
                .windows
                .iter()
                .position(|w| w.id == id && w.mapped)
                .ok_or("window is unavailable")?;
            if self.frame_window_attached(index) {
                let _ = self.reveal_frame_window(self.windows[index].id);
            }
            if !self.window_uses_output_coordinates(index) {
                let output = smithay::utils::Rectangle::new(
                    (view.x.round() as i32, view.y.round() as i32).into(),
                    (view.width.round() as i32, view.height.round() as i32).into(),
                );
                self.viewport_animation = None;
                self.viewport_offset = crate::compositor::navigation::reveal_offset(
                    self.window_canvas_rect(index),
                    output,
                    self.viewport_scale,
                );
            }
            let index = self.raise_window(index);
            if let Some(surface) = self.windows[index].surface.wl_surface() {
                self.set_keyboard_focus_to_window(index, surface);
            }
            self.retarget_canvas_preview_exit(Some(view), true);
        } else if result.kind == "item" {
            self.desktop_action("activate", &id)?;
        } else {
            self.issue_canvas_search_action(id, output)?;
        }
        self.request_redraw();
        Ok(())
    }
}
