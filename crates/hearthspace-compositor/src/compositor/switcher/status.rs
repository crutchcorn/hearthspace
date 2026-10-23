use super::*;
use crate::compositor::{CalloopData, shell_integration::ControlReply, windows::window_title};
use serde_json::{Value, json};

fn rect(r: Rect) -> Value {
    json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height})
}
impl App {
    pub(in crate::compositor) fn window_switcher_status(&self) -> Value {
        let owns_preview = self.canvas_preview.owner
            == Some(crate::compositor::canvas_preview::Owner::Switcher)
            && !self.session_locked();
        let scene:Vec<_>=self.windows.iter().filter_map(|w|{
            if !owns_preview { return None; }
            let pose=self.canvas_preview.poses.get(&w.id)?;
            let(display,opacity)=pose.current(std::time::Instant::now());
            Some(json!({"window_id":w.id,"canonical":rect(pose.natural),"display":rect(display),"opacity":opacity,"matches":pose.matches}))
        }).collect();
        let mut value = json!({"version":1,"revision":self.window_switcher.revision,"session":0,"active":false,"animating":owns_preview && self.canvas_preview_animating(),"source":null,"selected_window_id":null,"items":[],"window_ids":[],"output":null,"title":"","index":0,"count":0,"caption":null,"scene":scene});
        let Some(s) = self
            .window_switcher
            .session
            .as_ref()
            .filter(|_| !self.session_locked())
        else {
            return value;
        };
        let items:Vec<_>=s.ids.iter().filter_map(|id|{
            let index=self.windows.iter().position(|w|w.id==*id)?;
            let pose=self.canvas_preview.poses.get(id)?;
            let(p,_)=pose.current(std::time::Instant::now());
            let size=self.window_canvas_rect(index).size;
            let title=window_title(&self.windows[index].surface).unwrap_or_else(||"Untitled window".into());
            Some(json!({"window_id":id,"title":title,"x":p.x,"y":p.y,"width":p.width,"height":p.height,
                "preview_x":p.x,"preview_y":p.y,"preview_width":p.width,"preview_height":p.height,
                "scale":p.width/f64::from(size.w.max(1)),"canonical":rect(pose.natural),"target":rect(pose.to)}))
        }).collect();
        value["active"] = json!(true);
        value["session"] = json!(s.id);
        value["source"] = json!(match s.source {
            SwitcherSource::Keyboard => "keyboard",
            SwitcherSource::Gesture => "gesture",
        });
        value["selected_window_id"] = json!(s.selected);
        value["items"] = json!(items);
        value["window_ids"] = json!(s.ids);
        value["output"] = json!(s.output);
        value["title"] = json!(s.caption_title);
        value["index"] = json!(s.ids.iter().position(|id| *id == s.selected).unwrap_or(0) + 1);
        value["count"] = json!(s.ids.len());
        if let Some(r) = self.window_switcher_caption_rect() {
            value["caption"] = rect(r);
            value["caption"]["max_width"] = json!(s.caption_preview.width);
        }
        value
    }
}
impl CalloopData {
    pub(in crate::compositor) fn run_window_switcher_command(
        &mut self,
        text: &str,
    ) -> Option<ControlReply> {
        if text == "window-switcher-status" {
            let mut status = self.state.window_switcher_status();
            if !self.running || !self.capture_active() || self.state.session_locked() {
                status = json!({"version":1,"revision":self.state.window_switcher.revision,"active":false,"session":0,"title":"","index":0,"count":0,"output":null,"caption":null});
            }
            if let Some(object) = status.as_object_mut() {
                for key in ["scene", "items", "window_ids"] {
                    object.remove(key);
                }
            }
            return Some(ControlReply::Payload(serde_json::to_vec(&status).unwrap()));
        }
        let value = text.strip_prefix("window-switcher-caption-size ")?;
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Size {
            session: u64,
            revision: u64,
            width: f64,
        }
        let result = (|| -> Result<(), String> {
            if !self.running || !self.capture_active() || self.state.session_locked() {
                return Err("window switcher is inactive".into());
            }
            let value: Size =
                serde_json::from_str(value).map_err(|_| "invalid caption measurement")?;
            if value.revision != self.state.window_switcher.revision {
                return Err("stale caption revision".into());
            }
            let s = self
                .state
                .window_switcher
                .session
                .as_mut()
                .filter(|s| s.id == value.session)
                .ok_or("stale caption session")?;
            if !value.width.is_finite()
                || value.width <= 0.
                || value.width > s.caption_preview.width.max(1.)
            {
                return Err("invalid caption width".into());
            }
            s.caption_width = Some(value.width);
            self.state.request_redraw();
            Ok(())
        })();
        Some(match result {
            Ok(()) => ControlReply::Ok,
            Err(e) => ControlReply::Err(e),
        })
    }
}
