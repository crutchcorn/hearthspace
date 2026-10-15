use serde_json::{Value, json};
use smithay::utils::Rectangle;

use super::SwitcherSource;
use crate::compositor::{App, windows::window_title};

impl App {
    pub(in crate::compositor) fn window_switcher_status(&self) -> Value {
        let Some(session) = self.window_switcher.session.as_ref() else {
            return json!({"active":false, "source":null, "selected_window_id":null,
                "items":[], "window_ids":[], "output":null});
        };
        let items: Vec<_> = self
            .window_switcher_strip()
            .items
            .into_iter()
            .map(|item| {
                let title = self
                    .windows
                    .iter()
                    .find(|window| window.id == item.id)
                    .and_then(|window| window_title(&window.surface))
                    .unwrap_or_else(|| "Untitled window".into());
                json!({"window_id":item.id, "title":title,
                "x":item.rect.loc.x, "y":item.rect.loc.y,
                "width":item.rect.size.w, "height":item.rect.size.h,
                "preview_x":item.preview.loc.x, "preview_y":item.preview.loc.y,
                "preview_width":item.preview.size.w, "preview_height":item.preview.size.h,
                "scale":item.scale})
            })
            .collect();
        let output = self
            .capture_outputs()
            .into_iter()
            .find(|output| {
                Rectangle::new(
                    (output.logical_x, output.logical_y).into(),
                    (output.logical_width, output.logical_height).into(),
                ) == self.window_switcher_output()
            })
            .map(|output| output.name);
        let source = match session.source {
            SwitcherSource::Keyboard => "keyboard",
            SwitcherSource::Gesture => "gesture",
        };
        json!({"active":true, "source":source, "selected_window_id":session.selected,
            "items":items, "window_ids":session.ids, "output":output})
    }
}
