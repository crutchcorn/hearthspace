//! Held, live window selection over the normal canvas.
use cursor_icon::CursorIcon;
use smithay::{
    backend::input::InputTime,
    input::pointer::MotionEvent,
    utils::{Rectangle, SERIAL_COUNTER},
};

use super::{App, navigation::cycle_target, overview::render::Cache};
use crate::geometry::CanvasPoint;

mod layout;
mod render;
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
    cache: Cache,
}

struct Session {
    source: SwitcherSource,
    output: Option<String>,
    ids: Vec<u64>,
    selected: u64,
    previous_focus: Option<u64>,
    previous_offset: CanvasPoint,
    previous_scale: f64,
}

impl App {
    pub(super) fn window_switcher_active(&self) -> bool {
        self.window_switcher.session.is_some()
    }

    pub(super) fn window_switcher_source(&self) -> Option<SwitcherSource> {
        self.window_switcher
            .session
            .as_ref()
            .map(|session| session.source)
    }

    pub(super) fn window_switcher_selected_id(&self) -> Option<u64> {
        self.window_switcher
            .session
            .as_ref()
            .map(|session| session.selected)
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
            let Some(session) = self.window_switcher.session.as_mut() else {
                return Err("no mapped application windows".into());
            };
            session.selected = cycle_target(&session.ids, Some(session.selected), reverse)
                .expect("active switcher has windows");
        } else {
            if !self.navigation_available() {
                return Err("window switching unavailable during a modal interaction".into());
            }
            let mut ids = self.navigable_window_ids();
            ids.sort_unstable();
            let selected = cycle_target(&ids, self.focused_normal_window_id, reverse)
                .ok_or_else(|| "no mapped application windows".to_string())?;
            if source == SwitcherSource::Keyboard {
                self.interrupt_pinch_zoom();
            }
            self.advance_viewport_animation();
            self.viewport_animation = None;
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
            self.window_switcher.session = Some(Session {
                source,
                output,
                ids,
                selected,
                previous_focus: self
                    .keyboard
                    .current_focus()
                    .and_then(|focus| self.window_index_for_surface(focus.surface()))
                    .map(|index| self.windows[index].id),
                previous_offset: self.viewport_offset,
                previous_scale: self.viewport_scale,
            });
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
        }
        self.focus_switcher_window(self.window_switcher_selected_id().expect("active switcher"))
    }

    pub(super) fn finish_window_switcher(&mut self, source: SwitcherSource, cancelled: bool) {
        if self.window_switcher_source() == Some(source) {
            self.dismiss_window_switcher(cancelled);
        }
    }

    pub(super) fn dismiss_window_switcher(&mut self, cancelled: bool) {
        let Some(session) = self.window_switcher.session.take() else {
            return;
        };
        if session.source == SwitcherSource::Gesture {
            self.interrupt_window_switcher_gesture();
        }
        self.window_switcher.cache = Cache::default();
        if !self.session_locked() {
            if cancelled {
                self.start_owned_viewport_animation(
                    session.previous_offset,
                    session.previous_scale,
                );
                self.restore_switcher_focus(session.previous_focus);
            } else if self.focused_normal_window_id != Some(session.selected) {
                // The selected client may have disappeared between the most
                // recent event-loop reconciliation and the release event.
                let target = session.ids.into_iter().find(|id| {
                    self.windows
                        .iter()
                        .enumerate()
                        .any(|(index, window)| window.id == *id && self.is_navigable_window(index))
                });
                if let Some(id) = target {
                    let _ = self.focus_switcher_window(id);
                } else {
                    self.clear_keyboard_focus();
                }
            }
            let raw = self.raw_pointer_location;
            self.apply_pointer_motion(self.pointer_location, InputTime::now());
            self.raw_pointer_location = raw;
        } else {
            self.viewport_animation = None;
        }
        self.request_redraw();
    }

    /// Recovery cannot leave a partial pan behind after animations are stopped.
    pub(super) fn reset_window_switcher(&mut self) {
        let original = self
            .window_switcher
            .session
            .as_ref()
            .map(|session| (session.previous_offset, session.previous_scale));
        self.dismiss_window_switcher(true);
        if let Some((offset, scale)) = original {
            self.viewport_offset = offset;
            self.viewport_scale = scale;
            self.viewport_animation = None;
        }
    }

    pub(super) fn reconcile_window_switcher(&mut self) {
        if !self.window_switcher_active() {
            return;
        }
        if self.session_locked() {
            self.dismiss_window_switcher(true);
            return;
        }
        let live = self.navigable_window_ids();
        let session = self
            .window_switcher
            .session
            .as_mut()
            .expect("active switcher");
        let previous = session.selected;
        let replacement = surviving_selection(&mut session.ids, previous, &live);
        let Some(selected) = replacement else {
            self.advance_viewport_animation();
            self.viewport_animation = None;
            self.dismiss_window_switcher(false);
            return;
        };
        session.selected = selected;
        if selected != previous || self.focused_normal_window_id != Some(selected) {
            let _ = self.focus_switcher_window(selected);
        }
    }

    fn navigable_window_ids(&self) -> Vec<u64> {
        self.windows
            .iter()
            .enumerate()
            .filter(|(index, _)| self.is_navigable_window(*index))
            .map(|(_, window)| window.id)
            .collect()
    }

    fn restore_switcher_focus(&mut self, previous: Option<u64>) {
        let index = previous.and_then(|id| {
            self.windows
                .iter()
                .position(|window| window.id == id && window.mapped)
        });
        if let Some(index) = index {
            let index = self.raise_window(index);
            if let Some(surface) = self.windows[index].surface.wl_surface() {
                self.set_keyboard_focus_to_window(index, surface);
            }
        } else if previous.is_none() {
            self.clear_keyboard_focus();
        }
        // If the original client closed, retain the live selection's focus.
        // Cancellation still restores the view that preceded the interaction.
    }
}

/// Keep the session's original order; new clients join the next interaction.
/// If the selected client closes, select its next surviving neighbor.
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
