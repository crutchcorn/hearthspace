use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use super::{
    App,
    model::{Flight, gather_target},
};
use crate::compositor::{navigation::usable_output, windows::window_title};

pub(in crate::compositor) struct GatherCandidate {
    pub(in crate::compositor) id: u64,
    pub(in crate::compositor) title: String,
}

pub(in crate::compositor) struct GatherRequest {
    pub(in crate::compositor) request_id: u64,
    pub(in crate::compositor) state: Vec<String>,
    pub(in crate::compositor) candidates: Vec<GatherCandidate>,
    pub(in crate::compositor) cancel: Arc<AtomicBool>,
}

pub(super) struct PendingGather {
    pub(super) request_id: u64,
    pub(super) titles: BTreeMap<u64, String>,
    candidates: BTreeMap<u64, String>,
    pub(super) cancel: Arc<AtomicBool>,
}

impl App {
    pub(in crate::compositor) fn begin_group_gather(&mut self) -> Result<GatherRequest, String> {
        self.check_navigation_available()?;
        self.maintain_window_group();
        if self.window_group.selected.is_empty() {
            return Err("Select at least one window before gathering similar windows".into());
        }
        if self.window_group.pending.is_some() {
            return Err("Already gathering similar windows".into());
        }
        let titles = self.group_title_snapshot();
        let candidates: BTreeMap<_, _> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, window)| {
                self.is_navigable_window(*index)
                    && self.transient_parent_index(*index).is_none()
                    && !titles.contains_key(&window.id)
            })
            .map(|(_, window)| (window.id, window_title(&window.surface).unwrap_or_default()))
            .collect();
        self.window_group.invalidate();
        let cancel = Arc::new(AtomicBool::new(false));
        let request_id = self.window_group.generation;
        let request = GatherRequest {
            request_id,
            state: titles.values().cloned().collect(),
            candidates: candidates
                .iter()
                .map(|(&id, title)| GatherCandidate {
                    id,
                    title: title.clone(),
                })
                .collect(),
            cancel: cancel.clone(),
        };
        self.window_group.pending = Some(PendingGather {
            request_id,
            titles,
            candidates,
            cancel,
        });
        self.window_group.selecting = false;
        self.window_group.gesture = None;
        self.request_redraw();
        Ok(request)
    }

    pub(in crate::compositor) fn complete_group_gather(
        &mut self,
        request_id: u64,
        result: Result<Vec<u64>, String>,
    ) -> Result<(), String> {
        self.maintain_window_group();
        if self.session_locked()
            || !self
                .window_group
                .pending
                .as_ref()
                .is_some_and(|pending| pending.request_id == request_id)
        {
            return Err("Gather request no longer belongs to the current selection".into());
        }
        // Keep the original title snapshot and request alive until interactive
        // positioning finishes. A result must not add members halfway through a
        // group drag, or fight a candidate's client/compositor move or resize.
        if self.group_gather_layout_busy() {
            self.window_group.deferred_gather = Some((request_id, result));
            self.request_redraw();
            return Ok(());
        }
        let pending = self.window_group.pending.take().expect("matching request");
        let ids = match result {
            Ok(ids) => ids,
            Err(error) => {
                self.window_group.error = Some(error);
                self.request_redraw();
                return Ok(());
            }
        };
        let output = usable_output(self.group_output());
        let mut fan_index = self.window_group.selected.len();
        let initial_count = fan_index;
        for id in ids {
            if self.window_group.selected.contains(&id) {
                continue;
            }
            let Some(index) = self.windows.iter().position(|window| window.id == id) else {
                continue;
            };
            let title = window_title(&self.windows[index].surface).unwrap_or_default();
            if !self.is_navigable_window(index)
                || self.transient_parent_index(index).is_some()
                || pending.candidates.get(&id) != Some(&title)
            {
                continue;
            }
            self.grab_canvas_peek_target(index);
            let pinned = self.windows[index].presentation.pinned();
            let previous = self.window_screen_bounds(index);
            if pinned && self.restore_window_for_group(id).is_err() {
                continue;
            }
            self.windows[index].pending_resize = None;
            if pinned {
                let origin = crate::geometry::screen_to_canvas(
                    previous.loc.to_f64(),
                    self.viewport_offset,
                    self.viewport_scale,
                );
                self.windows[index].position = crate::geometry::CanvasPoint {
                    x: origin.x.round() as i32,
                    y: origin.y.round() as i32,
                };
            }
            let size = self.window_canvas_rect(index).size;
            let to = gather_target(
                fan_index,
                size,
                output,
                self.viewport_scale,
                self.viewport_offset,
            );
            fan_index += 1;
            self.window_group.flights.push(Flight {
                id,
                from: self.windows[index].position,
                to,
                // Keep the wave short even for many candidates.
                started: Instant::now()
                    + Duration::from_millis(((fan_index - initial_count) as u64 * 25).min(200)),
            });
            self.window_group.selected.insert(id);
            self.raise_window(index);
        }
        self.request_redraw();
        Ok(())
    }

    pub(super) fn group_title_snapshot(&self) -> BTreeMap<u64, String> {
        self.windows
            .iter()
            .filter(|window| self.window_group.selected.contains(&window.id))
            .map(|window| (window.id, window_title(&window.surface).unwrap_or_default()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::window_group::WindowGroup;

    #[test]
    fn changing_membership_cancels_pending_model_and_invalidates_generation() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut group = WindowGroup {
            generation: 4,
            selected: [1, 2].into(),
            pending: Some(PendingGather {
                request_id: 4,
                titles: BTreeMap::new(),
                candidates: BTreeMap::new(),
                cancel: cancel.clone(),
            }),
            ..Default::default()
        };
        group.replace_selection([1, 2].into());
        assert!(!cancel.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(group.generation, 4);
        group.replace_selection([1, 3].into());
        assert!(cancel.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(group.generation, 5);
        assert!(group.pending.is_none());
    }
}
