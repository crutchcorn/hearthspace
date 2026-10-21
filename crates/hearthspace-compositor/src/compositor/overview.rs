//! A modal, temporary view of live windows, independent of their canvas layout.
use std::time::{Duration, Instant};

use cursor_icon::CursorIcon;
use serde_json::{Value, json};
use smithay::{
    backend::input::InputTime,
    input::pointer::MotionEvent,
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
};

use super::{App, ViewportAnimation, windows::window_title};

pub(super) mod labels;
pub(super) mod layout;
pub(super) mod render;

const APPEAR_DURATION: Duration = Duration::from_millis(200);

#[derive(Default)]
pub(super) struct Overview {
    session: Option<Session>,
    cache: render::Cache,
}

struct Session {
    output: Option<String>,
    selected: Option<u64>,
    previous_focus: Option<u64>,
    paused_animation: Option<ViewportAnimation>,
    opened_at: Instant,
    appearance_finished: bool,
}

impl App {
    pub(super) fn overview_active(&self) -> bool {
        self.overview.session.is_some()
    }

    pub(super) fn set_overview(&mut self, active: bool) -> Result<(), String> {
        if active {
            self.open_overview()
        } else {
            self.close_overview();
            Ok(())
        }
    }

    pub(super) fn toggle_overview(&mut self) -> Result<(), String> {
        self.set_overview(!self.overview_active())
    }

    pub(super) fn open_overview(&mut self) -> Result<(), String> {
        if self.canvas_search_active() {
            return Err("overview unavailable during canvas search".into());
        }
        if self.session_locked() {
            return Err("overview unavailable while the session is locked".into());
        }
        if self.window_switcher_active() {
            return Err("overview unavailable during window switching".into());
        }
        if self.overview_active() {
            self.interrupt_overview_pinch();
            return Ok(());
        }
        if self.keyboard.is_grabbed()
            || self.pointer.is_grabbed()
            || self.drag.is_some()
            || self.resize.is_some()
        {
            return Err(
                "overview unavailable during an input grab or interactive move/resize".into(),
            );
        }
        self.interrupt_pinch_zoom();
        self.advance_viewport_animation();
        let output = self
            .capture_outputs()
            .into_iter()
            .find(|output| {
                Rectangle::new(
                    (output.logical_x, output.logical_y).into(),
                    (output.logical_width, output.logical_height).into(),
                )
                .contains(self.pointer_location.to_i32_round())
            })
            .map(|output| output.name);
        self.overview.session = Some(Session {
            output,
            selected: self.focused_normal_window_id,
            previous_focus: self
                .keyboard
                .current_focus()
                .and_then(|focus| self.window_index_for_surface(focus.surface()))
                .map(|index| self.windows[index].id),
            paused_animation: self.viewport_animation.take(),
            opened_at: Instant::now(),
            appearance_finished: false,
        });
        // Clients receive leave events before the modal view consumes input.
        self.clear_keyboard_focus();
        self.pointer.clone().motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::now(),
            },
        );
        self.pointer.clone().frame(self);
        self.cursor_surface = None;
        self.cursor_icon = CursorIcon::Default;
        self.reconcile_overview_selection();
        self.request_redraw();
        Ok(())
    }

    pub(super) fn close_overview(&mut self) {
        let session = self.overview.session.take();
        // End recognition even for an idempotent dismissal. Taking the modal
        // state first makes queued touchscreen releases consume their existing
        // overview press without activating a card during interruption.
        self.interrupt_overview_pinch();
        let Some(mut session) = session else {
            return;
        };
        if let Some(animation) = &mut session.paused_animation {
            animation.started_at += session.opened_at.elapsed();
        }
        self.viewport_animation = session.paused_animation;
        self.overview.cache = render::Cache::default();
        if !self.session_locked() {
            let live_ids: Vec<_> = self
                .windows
                .iter()
                .filter(|window| window.mapped)
                .map(|window| window.id)
                .collect();
            let fallback = (0..self.windows.len())
                .rev()
                .find(|index| self.is_navigable_window(*index))
                .map(|index| self.windows[index].id);
            let index = restoration_target(session.previous_focus, &live_ids, fallback)
                .and_then(|id| self.windows.iter().position(|window| window.id == id));
            if let Some(index) = index
                && let Some(surface) = self.windows[index].surface.wl_surface()
            {
                // Dismissal restores focus without changing z-order or the viewport.
                self.set_keyboard_focus_to_window(index, surface);
            }
            self.refresh_pointer_after_overview();
        }
        self.request_redraw();
    }

    pub(super) fn overview_window_at(&self, point: Point<f64, Logical>) -> Option<u64> {
        self.overview_items()
            .into_iter()
            .find(|item| item.rect.to_f64().contains(point))
            .map(|item| item.id)
    }

    pub(super) fn overview_hover(&mut self, point: Point<f64, Logical>) {
        if let Some(id) = self.overview_window_at(point) {
            let session = self
                .overview
                .session
                .as_mut()
                .expect("overview card requires active session");
            if session.selected != Some(id) {
                session.selected = Some(id);
                self.request_redraw();
            }
        }
    }

    pub(super) fn select_overview_window(&mut self, id: u64) -> Result<(), String> {
        if !self.overview_active() || !self.overview_items().iter().any(|item| item.id == id) {
            return Err(format!("window {id} is not in the overview"));
        }
        self.close_overview();
        let result = self.focus_window(id);
        self.refresh_pointer_after_overview();
        result
    }

    pub(super) fn overview_selection_cycle(&mut self, forward: bool) {
        let items = self.overview_items();
        let Some(session) = self.overview.session.as_mut() else {
            return;
        };
        session.selected = selection_target(
            &items.iter().map(|item| item.id).collect::<Vec<_>>(),
            session.selected,
            forward,
        );
        self.request_redraw();
    }

    pub(super) fn overview_activate_selected(&mut self) -> Result<(), String> {
        self.reconcile_overview_selection();
        let id = self
            .overview
            .session
            .as_ref()
            .and_then(|session| session.selected)
            .ok_or_else(|| "no window selected in overview".to_string())?;
        self.select_overview_window(id)
    }

    pub(super) fn advance_overview_animation(&mut self) {
        self.reconcile_overview_selection();
        if let Some(session) = self.overview.session.as_mut()
            && !session.appearance_finished
        {
            session.appearance_finished = session.opened_at.elapsed() >= APPEAR_DURATION;
            self.request_redraw();
        }
    }

    pub(super) fn overview_animating(&self) -> bool {
        self.overview
            .session
            .as_ref()
            .is_some_and(|session| !session.appearance_finished)
    }

    pub(super) fn overview_status(&self) -> Value {
        let items: Vec<_> = self.overview_items().into_iter().map(|item| {
            let title = self.windows.iter().find(|window| window.id == item.id)
                .and_then(|window| window_title(&window.surface)).unwrap_or_else(|| "Untitled window".into());
            json!({"window_id":item.id, "title":title,
                "x":item.rect.loc.x,"y":item.rect.loc.y,"width":item.rect.size.w,"height":item.rect.size.h,
                "preview_x":item.preview.loc.x,"preview_y":item.preview.loc.y,
                "preview_width":item.preview.size.w,"preview_height":item.preview.size.h,"scale":item.scale})
        }).collect();
        json!({"active":self.overview_active(), "animating":self.overview_animating(),
            "selected_window_id":self.overview.session.as_ref().and_then(|session| session.selected),
            "output":self.overview.session.as_ref().and_then(|_| self.capture_outputs().into_iter().find(|output| {
                Rectangle::new((output.logical_x, output.logical_y).into(), (output.logical_width, output.logical_height).into()) == self.overview_output()
            }).map(|output| output.name)),
            "items":items})
    }

    fn overview_output(&self) -> Rectangle<i32, Logical> {
        self.overview
            .session
            .as_ref()
            .and_then(|session| self.output_logical_rect_for_name(session.output.as_deref()))
            .unwrap_or_else(|| self.primary_output_logical_rect())
    }

    fn overview_items(&self) -> Vec<layout::Item> {
        if !self.overview_active() {
            return Vec::new();
        }
        let mut windows: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, _)| self.is_navigable_window(*index))
            .map(|(index, window)| (window.id, self.window_geometry_size(index)))
            .collect();
        windows.sort_unstable_by_key(|(id, _)| *id);
        layout::arrange(self.overview_output(), &windows)
    }

    fn reconcile_overview_selection(&mut self) {
        if !self.overview_active() {
            return;
        }
        let items = self.overview_items();
        let session = self.overview.session.as_mut().expect("active overview");
        if !items.iter().any(|item| Some(item.id) == session.selected) {
            session.selected = items.first().map(|item| item.id);
            self.request_redraw();
        }
    }

    fn refresh_pointer_after_overview(&mut self) {
        // Re-enter the real surface under a stationary pointer before the next
        // click, including after selection moved the viewport. Keep any raw
        // overshoot that the output constraint logic was tracking.
        let raw = self.raw_pointer_location;
        self.apply_pointer_motion(self.pointer_location, InputTime::now());
        self.raw_pointer_location = raw;
    }
}

fn restoration_target(
    previous: Option<u64>,
    live_ids: &[u64],
    fallback: Option<u64>,
) -> Option<u64> {
    previous.and_then(|id| {
        if live_ids.contains(&id) {
            Some(id)
        } else {
            fallback
        }
    })
}

fn selection_target(ids: &[u64], selected: Option<u64>, forward: bool) -> Option<u64> {
    if ids.is_empty() {
        return None;
    }
    let current = selected.and_then(|id| ids.iter().position(|candidate| *candidate == id));
    let index = match (current, forward) {
        (Some(index), true) => (index + 1) % ids.len(),
        (Some(index), false) => (index + ids.len() - 1) % ids.len(),
        (None, true) => 0,
        (None, false) => ids.len() - 1,
    };
    Some(ids[index])
}

#[cfg(test)]
mod tests {
    use super::{restoration_target, selection_target};

    #[test]
    fn dismissal_preserves_no_focus_and_restores_shell_or_live_window_focus() {
        assert_eq!(restoration_target(None, &[2, 4], Some(4)), None);
        assert_eq!(restoration_target(Some(2), &[2, 4], Some(4)), Some(2));
        assert_eq!(restoration_target(Some(3), &[2, 4], Some(4)), Some(4));
        assert_eq!(restoration_target(Some(3), &[], None), None);
    }

    #[test]
    fn selection_uses_live_ids_and_wraps_after_removal() {
        assert_eq!(selection_target(&[3, 8, 11], Some(11), true), Some(3));
        assert_eq!(selection_target(&[3, 8, 11], Some(3), false), Some(11));
        assert_eq!(selection_target(&[3, 11], Some(8), true), Some(3));
        assert_eq!(selection_target(&[], Some(8), false), None);
    }
}
