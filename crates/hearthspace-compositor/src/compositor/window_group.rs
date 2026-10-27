//! Compositor-owned transient groups; all model work lives on a worker thread.
use std::collections::BTreeSet;

use serde_json::{Value, json};
use smithay::utils::{Logical, Rectangle};

use super::{App, navigation::usable_output};
use model::{Flight, Gesture};

mod action_layout;
mod gather;
mod input;
mod lasso;
mod model;
mod render;
mod visibility;
mod worker;

pub(super) use gather::GatherRequest;

#[derive(Default)]
pub(super) struct WindowGroup {
    selecting: bool,
    selected: BTreeSet<u64>,
    keyboard_target: Option<u64>,
    output: Option<String>,
    generation: u64,
    pending: Option<gather::PendingGather>,
    deferred_gather: Option<(u64, Result<Vec<u64>, String>)>,
    error: Option<String>,
    pointer_held: bool,
    gesture: Option<Gesture>,
    flights: Vec<Flight>,
    cache: render::Cache,
    worker: Option<worker::GatherWorker>,
}

impl WindowGroup {
    fn invalidate(&mut self) {
        self.deferred_gather = None;
        if let Some(pending) = self.pending.take() {
            pending
                .cancel
                .store(true, std::sync::atomic::Ordering::Release);
        }
        self.generation = self.generation.wrapping_add(1);
        self.error = None;
    }

    fn replace_selection(&mut self, selected: BTreeSet<u64>) {
        if self.selected != selected {
            self.invalidate();
            self.selected = selected;
        }
    }
}

impl App {
    pub(in crate::compositor) fn disarm_group_selection(&mut self) {
        self.cancel_group_gesture();
        self.window_group.selecting = false;
        self.window_group.invalidate();
        self.layout_changed();
    }

    pub(in crate::compositor) fn select_group_tool(&mut self) {
        self.disarm_canvas_pan();
        self.disarm_canvas_search();
        self.disable_desktop_selection();
        self.interrupt_pinch_zoom();
        self.viewport_animation = None;
        self.window_group.selecting = true;
        self.window_group.invalidate();
        self.window_group.keyboard_target = self.focused_normal_window_id;
        self.window_group.output = self
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
        self.layout_changed();
    }

    pub(super) fn group_window_eligible(&self, index: usize) -> bool {
        self.is_navigable_window(index)
            && !self.window_uses_output_coordinates(index)
            && self.transient_parent_index(index).is_none()
    }

    pub(super) fn group_active(&self) -> bool {
        self.window_group.selecting || !self.window_group.selected.is_empty()
    }

    pub(super) fn group_selection_active(&self) -> bool {
        self.window_group.selecting
    }

    pub(super) fn group_interaction_active(&self) -> bool {
        self.window_group.gesture.is_some()
    }

    fn group_gather_layout_busy(&self) -> bool {
        self.group_interaction_active() || self.drag.is_some() || self.resize.is_some()
    }

    pub(super) fn toggle_group_selection(&mut self) -> Result<(), String> {
        self.disarm_canvas_search();
        self.check_canvas_tool_available()?;
        if self.window_group.selecting {
            self.disarm_group_selection();
        } else {
            self.select_group_tool();
        }
        self.request_redraw();
        Ok(())
    }

    pub(super) fn clear_window_group(&mut self) {
        self.window_group.invalidate();
        self.window_group.selecting = false;
        self.window_group.selected.clear();
        self.window_group.keyboard_target = None;
        self.window_group.gesture = None;
        self.window_group.flights.clear();
        self.layout_changed();
    }

    pub(super) fn cancel_group_gesture(&mut self) {
        if self.window_group.gesture.take().is_some() {
            self.request_redraw();
        }
    }

    pub(super) fn reset_window_group_input(&mut self) {
        self.clear_window_group();
        // Seat loss/lock discards physical releases. A later client press must
        // never inherit the old compositor-owned button's release suppression.
        self.window_group.pointer_held = false;
    }

    pub(super) fn cancel_group_window_motion(&mut self, id: u64) {
        self.window_group.flights.retain(|flight| flight.id != id);
    }

    pub(super) fn maintain_window_group(&mut self) {
        if self.session_locked() {
            if self.group_active() {
                self.clear_window_group();
            }
            return;
        }
        let selected = self
            .window_group
            .selected
            .iter()
            .copied()
            .filter(|id| {
                self.windows
                    .iter()
                    .position(|window| window.id == *id)
                    .is_some_and(|index| self.group_window_eligible(index))
            })
            .collect::<BTreeSet<_>>();
        if selected != self.window_group.selected {
            self.window_group.replace_selection(selected);
            self.window_group.gesture = None;
            self.request_redraw();
        }
        let stale = self
            .window_group
            .pending
            .as_ref()
            .is_some_and(|pending| pending.titles != self.group_title_snapshot());
        if stale {
            self.window_group.invalidate();
            self.request_redraw();
        }
        if self.window_switcher_active() {
            self.window_group.gesture = None;
            if self.window_group.pending.is_some() {
                self.window_group.invalidate();
                self.request_redraw();
            }
        }
        let mut flights = std::mem::take(&mut self.window_group.flights);
        let was_animating = !flights.is_empty();
        let mut arrived = Vec::new();
        let now = std::time::Instant::now();
        flights.retain(|flight| {
            let Some(index) = self
                .windows
                .iter()
                .position(|window| window.id == flight.id)
            else {
                return false;
            };
            if !self.group_window_eligible(index)
                || !self.window_group.selected.contains(&flight.id)
            {
                return false;
            }
            let (position, finished) = flight.position(now);
            self.set_group_window_position(index, position);
            if finished {
                arrived.push(flight.id);
            }
            !finished
        });
        if self.window_group.pending.is_some() || was_animating {
            self.request_redraw();
        }
        self.window_group.flights = flights;
        if !arrived.is_empty() {
            self.canvas_peek_arrangement_finished(&arrived);
        }
        if !self.group_gather_layout_busy()
            && let Some((request, result)) = self.window_group.deferred_gather.take()
        {
            let _ = self.complete_group_gather(request, result);
        }
    }

    pub(super) fn group_animating(&self) -> bool {
        !self.session_locked()
            && (self.window_group.pending.is_some() || !self.window_group.flights.is_empty())
    }

    pub(super) fn group_output(&self) -> Rectangle<i32, Logical> {
        self.output_logical_rect_for_name(self.window_group.output.as_deref())
            .unwrap_or_else(|| self.primary_output_logical_rect())
    }

    fn group_action_rect(&self) -> Rectangle<i32, Logical> {
        let bounds = self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, window)| self.window_group.selected.contains(&window.id))
            .map(|(index, _)| self.window_screen_bounds(index))
            .reduce(|a, b| a.merge(b));
        action_layout::action_rect(bounds, usable_output(self.group_output()))
    }

    pub(super) fn group_status(&self) -> Value {
        let action = self.group_action_rect();
        let lasso = match &self.window_group.gesture {
            Some(Gesture::Select { lasso }) => Some(
                lasso
                    .points()
                    .iter()
                    .map(|point| json!({"x":point.x, "y":point.y}))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        };
        json!({"version":1, "selecting":self.window_group.selecting,
            "selected":self.window_group.selected, "gathering":self.window_group.pending.is_some(),
            "waiting_for_drag":self.window_group.deferred_gather.is_some(),
            "error":self.window_group.error, "animating":!self.window_group.flights.is_empty(),
            "keyboard_target":self.window_group.keyboard_target, "lasso":lasso,
            "gather_button": (!self.window_group.selecting && !self.window_group.selected.is_empty()).then(|| json!({
                "x":action.loc.x,"y":action.loc.y,"width":action.size.w,"height":action.size.h}))})
    }

    pub(super) fn group_status_json(&self) -> Vec<u8> {
        serde_json::to_vec(&self.group_status()).expect("serializable window group")
    }

    pub(in crate::compositor) fn set_group_window_position(
        &mut self,
        index: usize,
        position: crate::geometry::CanvasPoint,
    ) {
        if let Some(mut bounds) = self.canvas_maximized_rect(index) {
            // A canvas-maximized member carries its visible rectangle and its
            // original restore home together. Pointer snapshots retain that home,
            // so use only the delta since the previous update here.
            let previous = self.windows[index].position;
            bounds.loc += (position.x - previous.x, position.y - previous.y).into();
            self.windows[index].presentation.maximized_bounds = Some(bounds);
        }
        self.windows[index].pending_resize = None;
        self.windows[index].position = position;
        #[cfg(feature = "xwayland")]
        if let super::ManagedSurface::X11(surface) = &self.windows[index].surface {
            if self.canvas_maximized_rect(index).is_some() {
                self.configure_window_presentation(index);
            } else {
                let _ = surface.configure(Some(Rectangle::new(
                    (position.x, position.y).into(),
                    surface.geometry().size,
                )));
            }
        }
    }
}
