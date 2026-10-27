use super::{App, Entry, Point};
use crate::compositor::{
    CalloopData,
    shell_integration::{ControlReply, command_socket_path},
};
use serde_json::{Value, json};
use std::{
    os::unix::net::UnixDatagram,
    time::{Duration, Instant},
};
fn send(value: Value) -> Result<(), String> {
    let socket = UnixDatagram::unbound().map_err(|e| e.to_string())?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    socket
        .send_to(
            &serde_json::to_vec(&value).unwrap(),
            hearthspace_common::shell_event_socket_path(&command_socket_path()),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn rect(r: super::Rect) -> Value {
    json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height})
}
impl App {
    pub(in crate::compositor) fn canvas_search_changed(&mut self) {
        self.canvas_search.revision = self.canvas_search.revision.wrapping_add(1);
        let _ = send(json!({"version":1,"action":"canvas-search-changed"}));
        self.request_redraw();
    }
    fn canvas_search_status(&self) -> Vec<u8> {
        let state = &self.canvas_search;
        let scene:Vec<_>=self.windows.iter().enumerate().filter_map(|(index,w)|{
            if self.canvas_preview.owner != Some(crate::compositor::canvas_preview::Owner::Search) { return None; }
            let pose=self.canvas_preview.poses.get(&w.id)?;let (display,alpha)=pose.current(Instant::now());
            let id=self.desktop_id(index).or_else(||self.canvas_search_result_identity(index).map(|(_,id)|id)).unwrap_or_else(||format!("window:{}",w.id));
            Some(json!({"id":id,"window_id":w.id,"canonical":rect(pose.natural),"display":rect(display),"opacity":alpha,"matches":pose.matches}))
        }).collect();
        let mut status = json!({"version":1,"revision":state.revision,"mode":state.mode,"active":false,"session":0,"output":null,"query":"","handwriting":false,"field":rect(Default::default()),"results_above":false,"handwriting_height":0,"result_count":0,"selected_id":null,"hovered_id":null,"results":[],"scene":scene});
        if let Some(s) = &state.session {
            status["active"] = json!(true);
            status["session"] = json!(s.id);
            status["output"] = json!(s.output);
            status["query"] = json!(s.query);
            status["field"] = rect(s.field);
            status["results_above"] = json!(s.above);
            status["result_count"] = json!(s.results.len());
            status["selected_id"] = json!(s.selected);
            status["hovered_id"] = json!(state.hovered);
            status["results"]=json!(s.results.iter().map(|r|json!({"id":r.id,"title":r.title,"kind":r.kind,"selected":Some(&r.id)==s.selected.as_ref()})).collect::<Vec<_>>());
        }
        serde_json::to_vec(&status).unwrap()
    }
    pub(super) fn issue_canvas_search_action(
        &mut self,
        id: String,
        output: Option<String>,
    ) -> Result<(), String> {
        let state = &mut self.canvas_search;
        let now = Instant::now();
        state.actions.retain(|(_, until, _, _)| now < *until);
        if state.actions.len() == 16 {
            state.actions.pop_front();
        }
        state.next_action = state.next_action.wrapping_add(1).max(1);
        let token = state.next_action;
        state
            .actions
            .push_back((token, now + Duration::from_secs(1), id, output));
        if let Err(e) = send(json!({"version":1,"action":"canvas-search-action","token":token})) {
            state.actions.pop_back();
            return Err(e);
        }
        Ok(())
    }
    fn check_canvas_search_session(&self, id: u64) -> Result<(), String> {
        self.canvas_search
            .session
            .as_ref()
            .is_some_and(|s| s.id == id)
            .then_some(())
            .ok_or_else(|| "canvas search session expired".into())
    }
    fn register_canvas_search(
        &mut self,
        offset: usize,
        entries: Vec<Entry>,
        desktop: bool,
    ) -> Result<(), String> {
        if offset > 4096
            || entries.len() > 4096 - offset
            || entries.iter().any(|e| {
                e.id.is_empty()
                    || e.id.len() > 1024
                    || e.title.len() > 4096
                    || e.search_text.len() > 8192
                    || e.id.chars().any(char::is_control)
                    || !["control:", "tool:", "app:", "file:"]
                        .iter()
                        .any(|prefix| e.id.starts_with(prefix))
            })
        {
            return Err("invalid canvas search registration".into());
        }
        let target = if desktop {
            &mut self.canvas_search.desktop_entries
        } else {
            &mut self.canvas_search.entries
        };
        if offset != 0 && offset != target.len() {
            return Err("stale canvas search registration offset".into());
        }
        let mut seen: std::collections::HashSet<_> = if offset == 0 {
            Default::default()
        } else {
            target.iter().map(|e| e.id.clone()).collect()
        };
        if entries.iter().any(|e| !seen.insert(e.id.clone())) {
            return Err("duplicate canvas search entry".into());
        }
        if offset == 0 {
            target.clear();
        }
        target.extend(entries);
        self.rebuild_canvas_search();
        Ok(())
    }
}
impl CalloopData {
    pub(in crate::compositor) fn run_canvas_search_command(
        &mut self,
        text: &str,
    ) -> Option<ControlReply> {
        if !text.starts_with("canvas-search-") {
            return None;
        }
        if !self.running || !self.capture_active() || self.state.session_locked() {
            self.state.reset_canvas_search();
            if text == "canvas-search-status" {
                return Some(ControlReply::Payload(self.state.canvas_search_status()));
            }
            return Some(ControlReply::Err("canvas search is inactive".into()));
        }
        let app = &mut self.state;
        let result = (|| -> Result<(), String> {
            if let Some(enabled) = text.strip_prefix("canvas-search-motion ") {
                match enabled {
                    "true" => app.canvas_preview.reduced_motion = false,
                    "false" => app.canvas_preview.reduced_motion = true,
                    _ => return Err("invalid motion preference".into()),
                }
                Ok(())
            } else if text == "canvas-search-tool" {
                app.check_canvas_tool_available()?;
                app.select_canvas_search_tool();
                Ok(())
            } else if text == "canvas-search-open" {
                app.open_canvas_search(None, None)
            } else if let Some(value) = text.strip_prefix("canvas-search-open ") {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Open {
                    x: Option<f64>,
                    y: Option<f64>,
                    output: Option<String>,
                }
                let value: Open =
                    serde_json::from_str(value).map_err(|_| "invalid canvas search anchor")?;
                let point = match (value.x, value.y) {
                    (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Some(Point { x, y }),
                    (None, None) => None,
                    _ => return Err("invalid canvas search anchor".into()),
                };
                app.open_canvas_search(point, value.output)
            } else if let Some(value) = text.strip_prefix("canvas-search-close ") {
                let id = value.parse().map_err(|_| "invalid canvas search session")?;
                app.check_canvas_search_session(id)?;
                app.close_canvas_search(true);
                Ok(())
            } else if let Some(value) = text.strip_prefix("canvas-search-update ") {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Update {
                    session: u64,
                    query: String,
                    #[serde(default)]
                    handwriting: bool,
                }
                let value: Update =
                    serde_json::from_str(value).map_err(|_| "invalid canvas search update")?;
                app.check_canvas_search_session(value.session)?;
                if value.handwriting
                    || value.query.len() > hearthspace_common::canvas_search::MAX_QUERY_BYTES
                {
                    return Err("invalid canvas search query".into());
                }
                if app.canvas_search.session.as_ref().unwrap().query != value.query {
                    app.canvas_search.session.as_mut().unwrap().selected = None;
                }
                app.canvas_search.session.as_mut().unwrap().query = value.query;
                app.rebuild_canvas_search();
                app.canvas_search_changed();
                Ok(())
            } else if let Some(value) = text.strip_prefix("canvas-search-step ") {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Step {
                    session: u64,
                    step: i32,
                }
                let v: Step =
                    serde_json::from_str(value).map_err(|_| "invalid canvas search step")?;
                app.check_canvas_search_session(v.session)?;
                if !matches!(v.step, -1 | 1) {
                    return Err("step must be -1 or 1".into());
                }
                app.step_canvas_search(v.step);
                Ok(())
            } else if let Some(value) = text.strip_prefix("canvas-search-select ") {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Select {
                    session: u64,
                    id: String,
                }
                let v: Select =
                    serde_json::from_str(value).map_err(|_| "invalid canvas search selection")?;
                app.check_canvas_search_session(v.session)?;
                if !app
                    .canvas_search
                    .session
                    .as_ref()
                    .unwrap()
                    .results
                    .iter()
                    .any(|r| r.id == v.id)
                {
                    return Err("search result is unavailable".into());
                }
                app.canvas_search.session.as_mut().unwrap().selected = Some(v.id);
                app.canvas_search_changed();
                Ok(())
            } else if let Some(value) = text.strip_prefix("canvas-search-activate ") {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Activate {
                    session: u64,
                    id: Option<String>,
                }
                let v: Activate =
                    serde_json::from_str(value).map_err(|_| "invalid canvas search activation")?;
                app.check_canvas_search_session(v.session)?;
                app.activate_canvas_search(v.id)
            } else if let Some(value) = text.strip_prefix("canvas-search-desktop-register ") {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Register {
                    offset: usize,
                    entries: Vec<Entry>,
                }
                let v: Register = serde_json::from_str(value)
                    .map_err(|_| "invalid canvas desktop registration")?;
                if v.entries
                    .iter()
                    .any(|e| !(e.id.starts_with("app:") || e.id.starts_with("file:")))
                {
                    return Err("invalid desktop metadata".into());
                }
                app.register_canvas_search(v.offset, v.entries, true)
            } else if let Some(value) = text.strip_prefix("canvas-search-register ") {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Register {
                    offset: usize,
                    entries: Vec<Entry>,
                }
                let v: Register = serde_json::from_str(value)
                    .map_err(|_| "invalid canvas search registration")?;
                app.register_canvas_search(v.offset, v.entries, false)
            } else {
                Err("invalid canvas search command".into())
            }
        })();
        if text == "canvas-search-status" {
            return Some(ControlReply::Payload(app.canvas_search_status()));
        }
        if let Some(token) = text
            .strip_prefix("canvas-search-event-accept ")
            .and_then(|s| s.parse::<u64>().ok())
        {
            let now = Instant::now();
            app.canvas_search
                .actions
                .retain(|(_, until, _, _)| now < *until);
            return Some(
                if let Some(i) = app
                    .canvas_search
                    .actions
                    .iter()
                    .position(|(t, _, _, _)| *t == token)
                {
                    let (_, _, id, output) = app.canvas_search.actions.remove(i).unwrap();
                    ControlReply::Payload(
                        serde_json::to_vec(&json!({"id":id,"output":output})).unwrap(),
                    )
                } else {
                    ControlReply::Err(
                        "canvas search action expired, cancelled or already accepted".into(),
                    )
                },
            );
        }
        Some(result.map_or_else(ControlReply::Err, |()| ControlReply::Ok))
    }
}
