//! Held MRU selection uses the same live canvas presentation as search.
use smithay::utils::Rectangle;

use super::{App, navigation::cycle_target};
use hearthspace_common::canvas_search::Rect;

mod caption;
mod scene;
mod status;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SwitcherSource {
    Keyboard,
    Gesture,
}

#[derive(Default)]
pub(super) struct WindowSwitcher {
    session: Option<Session>,
    sequence: u64,
    revision: u64,
}

struct Session {
    id: u64,
    source: SwitcherSource,
    output: Option<String>,
    view: Rect,
    ids: Vec<u64>,
    selected: u64,
    previous_focus: Option<u64>,
    caption_width: Option<f64>,
    caption_title: String,
    caption_preview: Rect,
}

impl App {
    pub(super) fn window_switcher_active(&self) -> bool {
        self.window_switcher.session.is_some()
    }
    pub(super) fn window_switcher_source(&self) -> Option<SwitcherSource> {
        self.window_switcher.session.as_ref().map(|s| s.source)
    }
    pub(super) fn window_switcher_selected_id(&self) -> Option<u64> {
        self.window_switcher.session.as_ref().map(|s| s.selected)
    }
    pub(super) fn cycle_window_switcher(
        &mut self,
        source: SwitcherSource,
        reverse: bool,
    ) -> Result<(), String> {
        if let Some(owner) = self.window_switcher_source() {
            if owner != source {
                return Err("another input source owns the window switcher".into());
            }
            self.reconcile_window_switcher();
            let s = self
                .window_switcher
                .session
                .as_mut()
                .ok_or("no mapped application windows")?;
            s.selected = cycle_target(&s.ids, Some(s.selected), reverse).unwrap();
        } else {
            if self.canvas_search_active() {
                self.close_canvas_search(true);
            }
            self.check_navigation_available()?;
            let mut ids = self.navigable_window_ids();
            if let Some(index) = ids
                .iter()
                .position(|id| Some(*id) == self.focused_normal_window_id)
            {
                let current = ids.remove(index);
                ids.insert(0, current);
            }
            let selected = cycle_target(&ids, ids.first().copied(), reverse)
                .ok_or("no mapped application windows")?;
            if source == SwitcherSource::Keyboard {
                self.interrupt_pinch_zoom();
            }
            self.cancel_pending_search();
            self.cancel_desktop_gesture();
            self.clear_window_group();
            if self.active_shell_panel_index().is_some() {
                self.dismiss_active_shell_panel();
                self.restore_focus_after_shell_panel();
            }
            self.advance_viewport_animation();
            self.viewport_animation = None;
            let outputs = self.capture_outputs();
            let output = outputs
                .iter()
                .find(|o| {
                    Rectangle::new(
                        (o.logical_x, o.logical_y).into(),
                        (o.logical_width, o.logical_height).into(),
                    )
                    .contains(self.pointer_location.to_i32_round())
                })
                .or(outputs.first())
                .ok_or("no output available")?;
            let view = Rect {
                x: f64::from(output.logical_x),
                y: f64::from(output.logical_y),
                width: f64::from(output.logical_width),
                height: f64::from(output.logical_height),
            };
            self.window_switcher.sequence = self.window_switcher.sequence.wrapping_add(1).max(1);
            self.canvas_preview.owner = Some(super::canvas_preview::Owner::Switcher);
            self.window_switcher.session = Some(Session {
                id: self.window_switcher.sequence,
                source,
                output: Some(output.name.clone()),
                view,
                ids,
                selected,
                previous_focus: self
                    .keyboard
                    .current_focus()
                    .and_then(|f| self.window_index_for_surface(f.surface()))
                    .map(|i| self.windows[i].id),
                caption_width: None,
                caption_title: String::new(),
                caption_preview: Rect::default(),
            });
        }
        self.rebuild_window_switcher();
        self.window_switcher_changed();
        self.apply_pointer_motion(
            self.pointer_location,
            smithay::backend::input::InputTime::now(),
        );
        Ok(())
    }
    pub(super) fn finish_window_switcher(&mut self, source: SwitcherSource, cancelled: bool) {
        if self.window_switcher_source() == Some(source) {
            self.dismiss_window_switcher(cancelled);
        }
    }
    pub(super) fn select_window_switcher(&mut self, id: u64) -> Result<(), String> {
        let s = self
            .window_switcher
            .session
            .as_mut()
            .ok_or("window switcher is inactive")?;
        if !s.ids.contains(&id) {
            return Err("window is not a switcher result".into());
        }
        s.selected = id;
        self.dismiss_window_switcher(false);
        Ok(())
    }
    pub(super) fn dismiss_window_switcher(&mut self, cancelled: bool) {
        // Clear the modal owner before touch cleanup can deliver queued releases.
        let session = self.window_switcher.session.take();
        self.interrupt_switcher_pinch();
        let Some(session) = session else {
            return;
        };
        if session.source == SwitcherSource::Gesture {
            self.interrupt_window_switcher_gesture();
        }
        self.cancel_window_switcher_touch();
        for (i, w) in self.windows.iter().enumerate() {
            if self.is_window_switcher_caption(i) {
                w.surface.send_close();
            }
        }
        if !self.session_locked() {
            if cancelled {
                self.restore_switcher_focus(session.previous_focus);
            } else if let Some(index) = self
                .windows
                .iter()
                .enumerate()
                .find(|(i, w)| w.id == session.selected && self.is_navigable_window(*i))
                .map(|(i, _)| i)
            {
                let pending_focus_view = self.window_focus_view_pending_for_focus(index);
                if !pending_focus_view && self.frame_window_attached(index) {
                    let _ = self.reveal_frame_window(self.windows[index].id);
                }
                if !pending_focus_view && !self.window_uses_output_coordinates(index) {
                    self.viewport_animation = None;
                    self.viewport_offset = super::navigation::reveal_offset(
                        self.window_canvas_rect(index),
                        Rectangle::new(
                            (session.view.x.round() as i32, session.view.y.round() as i32).into(),
                            (
                                session.view.width.round() as i32,
                                session.view.height.round() as i32,
                            )
                                .into(),
                        ),
                        self.viewport_scale,
                    );
                }
                let index = self.raise_window(index);
                if let Some(surface) = self.windows[index].surface.wl_surface() {
                    self.set_keyboard_focus_to_window(index, surface);
                }
            } else {
                self.restore_switcher_focus(session.previous_focus);
            }
            self.retarget_canvas_preview_exit(Some(session.view), !cancelled);
            let raw = self.raw_pointer_location;
            self.apply_pointer_motion(
                self.pointer_location,
                smithay::backend::input::InputTime::now(),
            );
            self.raw_pointer_location = raw;
        } else {
            self.canvas_preview.poses.clear();
        }
        self.window_switcher_changed();
    }
    pub(super) fn reset_window_switcher(&mut self) {
        self.dismiss_window_switcher(true);
        if self.canvas_preview.owner == Some(crate::compositor::canvas_preview::Owner::Switcher) {
            self.canvas_preview.poses.clear();
            self.canvas_preview.owner = None;
        }
    }
    pub(super) fn reconcile_window_switcher(&mut self) {
        let Some(s) = self.window_switcher.session.as_ref() else {
            return;
        };
        if self.session_locked()
            || self
                .output_logical_rect_for_name(s.output.as_deref())
                .is_none()
        {
            self.dismiss_window_switcher(true);
            return;
        }
        let live = self.navigable_window_ids();
        let s = self.window_switcher.session.as_mut().unwrap();
        let previous = s.selected;
        let previous_count = s.ids.len();
        let Some(selected) = surviving_selection(&mut s.ids, previous, &live) else {
            self.dismiss_window_switcher(true);
            return;
        };
        s.selected = selected;
        if previous_count != s.ids.len() || previous != selected {
            self.window_switcher_changed();
        }
        self.rebuild_window_switcher();
    }
    fn navigable_window_ids(&self) -> Vec<u64> {
        self.windows
            .iter()
            .enumerate()
            .rev()
            .filter(|(i, _)| self.is_navigable_window(*i))
            .map(|(_, w)| w.id)
            .collect()
    }
    fn restore_switcher_focus(&mut self, previous: Option<u64>) {
        if let Some(index) =
            previous.and_then(|id| self.windows.iter().position(|w| w.id == id && w.mapped))
        {
            if let Some(surface) = self.windows[index].surface.wl_surface() {
                self.set_keyboard_focus_to_window(index, surface);
            }
        } else {
            self.clear_keyboard_focus();
        }
    }
}

/// Keep the original order; new clients join only the next interaction.
fn surviving_selection(ids: &mut Vec<u64>, selected: u64, live: &[u64]) -> Option<u64> {
    let current = ids.iter().position(|id| *id == selected).unwrap_or(0);
    let replacement = ids[current..]
        .iter()
        .chain(&ids[..current])
        .copied()
        .find(|id| live.contains(id));
    ids.retain(|id| live.contains(id));
    replacement
}
