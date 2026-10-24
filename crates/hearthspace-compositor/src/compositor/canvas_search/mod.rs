//! Temporary, compositor-owned canvas search presentation. Stored coordinates never change.
use super::{
    App, ManagedWindowKind,
    windows::{window_app_id, window_title},
};
use hearthspace_common::canvas_search::{Point, Rect};
use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
};

mod input;
mod motion;
mod protocol;
mod scene;

#[derive(Default)]
pub(super) struct State {
    mode: bool,
    sequence: u64,
    revision: u64,
    hovered: Option<String>,
    session: Option<Session>,
    entries: Vec<Entry>,
    desktop_entries: Vec<Entry>,
    buttons: HashMap<u32, (Option<String>, Point)>,
    field_buttons: std::collections::HashSet<u32>,
    actions: VecDeque<(u64, Instant, String, Option<String>)>,
    next_action: u64,
}

struct Session {
    id: u64,
    output: Option<String>,
    view: Rect,
    anchor: Point,
    query: String,
    field: Rect,
    above: bool,
    selected: Option<String>,
    results: Vec<ResultItem>,
    previous_focus: Option<u64>,
    signature: Option<u64>,
}

#[derive(Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    title: String,
    search_text: String,
}

#[derive(Clone)]
struct ResultItem {
    id: String,
    title: String,
    kind: &'static str,
    window: Option<u64>,
    natural: Rect,
    search_text: String,
}

impl App {
    pub(in crate::compositor) fn canvas_search_active(&self) -> bool {
        self.canvas_search.session.is_some()
    }
    pub(in crate::compositor) fn canvas_search_window_selected(&self, id: u64) -> bool {
        self.canvas_search.session.as_ref().is_some_and(|s| {
            s.selected
                .as_ref()
                .is_some_and(|selected| selected == &format!("window:{id}"))
        })
    }
    pub(in crate::compositor) fn is_canvas_search_panel(&self, index: usize) -> bool {
        window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(hearthspace_common::canvas_search_from_app_id)
            .is_some()
    }
    pub(in crate::compositor) fn canvas_search_result_identity(
        &self,
        index: usize,
    ) -> Option<(u64, String)> {
        window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(hearthspace_common::canvas_search_result_from_app_id)
    }
    pub(in crate::compositor) fn canvas_search_result_visible(&self, index: usize) -> bool {
        let Some((session, id)) = self.canvas_search_result_identity(index) else {
            return false;
        };
        self.canvas_search.session.as_ref().is_some_and(|s| {
            (session == 0 || session == s.id)
                && s.results
                    .iter()
                    .any(|result| result.id == id && result.window == Some(self.windows[index].id))
        })
    }
    pub(in crate::compositor) fn canvas_search_result_emphasized(&self, index: usize) -> bool {
        let Some((_, id)) = self.canvas_search_result_identity(index) else {
            return false;
        };
        self.canvas_search_result_visible(index)
            && self.canvas_search.session.as_ref().is_some_and(|s| {
                s.selected.as_ref() == Some(&id) || self.canvas_search.hovered.as_ref() == Some(&id)
            })
    }
    pub(in crate::compositor) fn canvas_search_panel_allowed(&self, index: usize) -> bool {
        let identity = window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(hearthspace_common::canvas_search_from_app_id);
        self.canvas_search
            .session
            .as_ref()
            .is_some_and(|s| identity == Some((s.id, s.output.clone())))
    }
    pub(in crate::compositor) fn canvas_search_surface_origin(
        &self,
        index: usize,
    ) -> Option<smithay::utils::Point<i32, smithay::utils::Logical>> {
        let rect = if self.is_canvas_search_panel(index) {
            self.canvas_search_panel_allowed(index)
                .then(|| self.canvas_search.session.as_ref().unwrap().field)?
        } else {
            self.canvas_search_result_identity(index)?;
            self.canvas_preview_pose(index)?.0
        };
        Some(
            smithay::utils::Point::from((rect.x.round() as i32, rect.y.round() as i32))
                - self.window_surface_geometry_loc(index),
        )
    }
    pub(in crate::compositor) fn open_canvas_search(
        &mut self,
        anchor: Option<Point>,
        output: Option<String>,
    ) -> Result<(), String> {
        if self.canvas_search_active() {
            self.close_canvas_search(true);
        }
        self.check_navigation_available()?;
        self.leave_window_focus_views();
        let outputs = self.capture_outputs();
        let target = outputs
            .iter()
            .find(|o| output.as_ref().is_some_and(|name| name == &o.name))
            .or_else(|| {
                outputs.iter().find(|o| {
                    self.pointer_location.x >= f64::from(o.logical_x)
                        && self.pointer_location.x < f64::from(o.logical_x + o.logical_width)
                        && self.pointer_location.y >= f64::from(o.logical_y)
                        && self.pointer_location.y < f64::from(o.logical_y + o.logical_height)
                })
            })
            .or(outputs.first())
            .ok_or("no output available")?;
        let view = Rect {
            x: f64::from(target.logical_x),
            y: f64::from(target.logical_y),
            width: f64::from(target.logical_width),
            height: f64::from(target.logical_height),
        };
        let output = Some(target.name.clone());
        let (desktop_release, previous_focus) = self.prepare_canvas_search_entry();
        self.canvas_search.mode = true;
        self.canvas_search.sequence = self.canvas_search.sequence.wrapping_add(1).max(1);
        let id = self.canvas_search.sequence;
        self.canvas_preview.owner = Some(super::canvas_preview::Owner::Search);
        self.canvas_search.session = Some(Session {
            id,
            output: output.clone(),
            view,
            anchor: anchor.unwrap_or(Point {
                x: view.x + view.width / 2.,
                y: view.y + view.height / 3.,
            }),
            query: String::new(),
            field: Rect::default(),
            above: false,
            selected: None,
            results: Vec::new(),
            previous_focus,
            signature: None,
        });
        if desktop_release {
            self.canvas_search.buttons.insert(
                0x110,
                (
                    Some(String::new()),
                    Point {
                        x: self.pointer_location.x,
                        y: self.pointer_location.y,
                    },
                ),
            );
        }
        self.reserve_canvas_search_input(id, output);
        self.rebuild_canvas_search();
        self.apply_pointer_motion(
            self.pointer_location,
            smithay::backend::input::InputTime::now(),
        );
        Ok(())
    }
    pub(in crate::compositor) fn close_canvas_search(&mut self, restore: bool) {
        let Some(session) = self.canvas_search.session.take() else {
            return;
        };
        self.cancel_pending_search();
        self.canvas_search.hovered = None;
        self.clear_keyboard_focus();
        for (index, window) in self.windows.iter().enumerate() {
            if self.is_canvas_search_panel(index) {
                window.surface.send_close();
            }
            if let Some((session, _)) = self.canvas_search_result_identity(index) {
                // Preview cards are inert shell resources. Reusing their buffers
                // avoids per-window renderer startup on every search opening.
                self.canvas_preview.poses.remove(&window.id);
                if session != 0 {
                    window.surface.send_close();
                }
            }
        }
        if restore
            && let Some(index) = self
                .windows
                .iter()
                .position(|w| Some(w.id) == session.previous_focus && w.mapped)
            && let Some(surface) = self.windows[index].surface.wl_surface()
        {
            self.set_keyboard_focus_to_window(index, surface);
        }
        self.retarget_canvas_preview_exit(Some(session.view), false);
        self.canvas_search_changed();
    }
    pub(in crate::compositor) fn reset_canvas_search(&mut self) {
        self.close_canvas_search(false);
        if self.canvas_preview.owner == Some(crate::compositor::canvas_preview::Owner::Search) {
            self.canvas_preview.poses.clear();
            self.canvas_preview.owner = None;
        }
        self.canvas_search.buttons.clear();
        self.canvas_search.field_buttons.clear();
        self.canvas_search.actions.clear();
        self.canvas_search.mode = false;
    }
    pub(in crate::compositor) fn disarm_canvas_search(&mut self) {
        self.close_canvas_search(true);
        if self.canvas_search.mode {
            self.canvas_search.mode = false;
            self.canvas_search_changed();
        }
    }
    fn step_canvas_search(&mut self, step: i32) {
        let Some(s) = self.canvas_search.session.as_mut() else {
            return;
        };
        if s.results.is_empty() {
            return;
        }
        let current = s
            .results
            .iter()
            .position(|r| Some(&r.id) == s.selected.as_ref())
            .unwrap_or(0);
        let next = (current as i64 + i64::from(step)).rem_euclid(s.results.len() as i64) as usize;
        s.selected = Some(s.results[next].id.clone());
        self.canvas_search_changed();
    }
    fn activate_canvas_search(&mut self, id: Option<String>) -> Result<(), String> {
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
                self.viewport_offset = super::navigation::reveal_offset(
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
