//! Frame-batched touchscreen recognition, independent of compositor state.
use std::collections::HashMap;

use smithay::{
    backend::input::TouchSlot,
    utils::{Logical, Point},
};

type Position = Point<f64, Logical>;
type Actions = Vec<(String, TouchAction)>;
const DRAG_THRESHOLD: f64 = 8.0;

#[derive(Debug, PartialEq)]
pub(in crate::compositor) enum TouchAction {
    Hover(Position),
    Press(Position),
    Motion(Position),
    Release,
    Tap(Position),
    BeginPinch { anchor: Position },
    UpdatePinch { scale: f64 },
    EndPinch { cancelled: bool },
}

struct Contact {
    origin: Position,
    position: Position,
}

#[derive(Default)]
enum Mode {
    #[default]
    Pending,
    Dragging,
    Multiple,
    Pinching {
        radius: f64,
    },
    Suppressed,
}

#[derive(Default)]
pub(in crate::compositor) struct TouchTracker {
    device: Option<String>,
    contacts: HashMap<TouchSlot, Contact>,
    mode: Mode,
    pending: Actions,
    dirty: bool,
}

impl TouchTracker {
    pub(in crate::compositor) fn has_contacts(&self) -> bool {
        !self.contacts.is_empty()
    }

    pub(in crate::compositor) fn down(
        &mut self,
        device: &str,
        slot: TouchSlot,
        position: Position,
    ) {
        if self.device.as_deref().is_some_and(|owner| owner != device)
            || self.contacts.contains_key(&slot)
        {
            return;
        }
        self.device.get_or_insert_with(|| device.to_owned());
        self.contacts.insert(
            slot,
            Contact {
                origin: position,
                position,
            },
        );
        self.dirty = true;
        match self.mode {
            Mode::Pending if self.contacts.len() == 1 => {
                if finite(position) {
                    self.emit(TouchAction::Hover(position));
                }
            }
            Mode::Pending => self.mode = Mode::Multiple,
            Mode::Dragging => {
                self.emit(TouchAction::Release);
                self.mode = Mode::Suppressed;
            }
            Mode::Pinching { .. } => {
                self.emit(TouchAction::EndPinch { cancelled: true });
                self.mode = Mode::Suppressed;
            }
            _ => {}
        }
        // Remember a fourth contact even if it disappears before the frame.
        if self.contacts.len() > 3 {
            self.mode = Mode::Suppressed;
        }
    }

    pub(in crate::compositor) fn motion(
        &mut self,
        device: &str,
        slot: TouchSlot,
        position: Position,
    ) {
        if self.device.as_deref() != Some(device) {
            return;
        }
        if let Some(contact) = self.contacts.get_mut(&slot) {
            contact.position = position;
            self.dirty = true;
        }
    }

    pub(in crate::compositor) fn up(&mut self, device: &str, slot: TouchSlot) {
        if self.device.as_deref() != Some(device) || !self.contacts.contains_key(&slot) {
            return;
        }
        if self.dirty
            && matches!(
                self.mode,
                Mode::Pending | Mode::Dragging | Mode::Pinching { .. }
            )
        {
            // An up may share a frame with the final motion of all three contacts.
            self.recognize();
        }
        let contact = self.contacts.remove(&slot).unwrap();
        match self.mode {
            Mode::Pending if finite(contact.origin) && finite(contact.position) => {
                self.emit(TouchAction::Tap(contact.position))
            }
            Mode::Dragging => self.emit(TouchAction::Release),
            Mode::Pinching { .. } => self.emit(TouchAction::EndPinch { cancelled: false }),
            _ => {}
        }
        self.mode = Mode::Suppressed;
        self.finish_empty();
    }

    pub(in crate::compositor) fn cancel(&mut self, device: &str, slot: TouchSlot) {
        if self.device.as_deref() != Some(device)
            || (i32::from(slot) >= 0 && !self.contacts.contains_key(&slot))
        {
            return;
        }
        self.stop(true);
        if i32::from(slot) < 0 {
            self.contacts.clear();
        } else {
            self.contacts.remove(&slot);
        }
        self.finish_empty();
    }

    pub(in crate::compositor) fn frame(&mut self, device: &str) -> Actions {
        if self.device.as_deref() == Some(device) {
            self.recognize();
        }
        let (actions, pending) = self
            .pending
            .drain(..)
            .partition(|(owner, _)| owner == device);
        self.pending = pending;
        actions
    }

    pub(in crate::compositor) fn frame_all(&mut self) -> Actions {
        self.recognize();
        std::mem::take(&mut self.pending)
    }

    pub(in crate::compositor) fn interrupt(&mut self) -> Actions {
        // External input keeps the current zoom. Late touch events cannot resume it.
        self.pending
            .retain(|(_, action)| matches!(action, TouchAction::Release));
        self.stop(false);
        self.finish_empty();
        std::mem::take(&mut self.pending)
    }

    pub(in crate::compositor) fn reset(&mut self) -> Actions {
        self.discard_pending_input();
        self.stop(true);
        self.contacts.clear();
        self.finish_empty();
        std::mem::take(&mut self.pending)
    }

    pub(in crate::compositor) fn remove_device(&mut self, device: &str) -> Actions {
        self.discard_pending_input_for(device);
        if self.device.as_deref() == Some(device) {
            self.stop(true);
            self.contacts.clear();
            self.finish_empty();
        }
        self.frame(device)
    }

    fn recognize(&mut self) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        match self.mode {
            Mode::Pending | Mode::Dragging => self.single_motion(),
            Mode::Multiple if self.contacts.len() == 3 => {
                if let Some((anchor, radius)) = self.geometry() {
                    self.mode = Mode::Pinching { radius };
                    self.emit(TouchAction::BeginPinch { anchor });
                }
            }
            Mode::Pinching { radius } => {
                if let Some((_, current)) = self.geometry() {
                    let scale = current / radius;
                    if scale.is_finite() && scale > 0.0 {
                        self.emit(TouchAction::UpdatePinch { scale });
                    }
                }
            }
            _ => {}
        }
    }

    fn single_motion(&mut self) {
        let Some(contact) = self.contacts.values().next() else {
            return;
        };
        let (origin, position) = (contact.origin, contact.position);
        if !finite(origin) || !finite(position) {
            return;
        }
        if matches!(self.mode, Mode::Pending)
            && (position.x - origin.x).hypot(position.y - origin.y) >= DRAG_THRESHOLD
        {
            self.mode = Mode::Dragging;
            self.emit(TouchAction::Press(origin));
        }
        if matches!(self.mode, Mode::Dragging) {
            self.emit(TouchAction::Motion(position));
        }
    }

    fn geometry(&self) -> Option<(Position, f64)> {
        if self.contacts.len() != 3
            || self
                .contacts
                .values()
                .any(|contact| !finite(contact.position))
        {
            return None;
        }
        let anchor = self
            .contacts
            .values()
            .fold(Position::from((0.0, 0.0)), |sum, contact| {
                sum + Point::from((contact.position.x / 3.0, contact.position.y / 3.0))
            });
        let radius = (self
            .contacts
            .values()
            .map(|contact| {
                let delta = contact.position - anchor;
                (delta.x * delta.x + delta.y * delta.y) / 3.0
            })
            .sum::<f64>())
        .sqrt();
        (finite(anchor) && radius.is_finite() && radius > 1e-6).then_some((anchor, radius))
    }

    fn stop(&mut self, cancelled: bool) {
        match self.mode {
            Mode::Dragging => self.emit(TouchAction::Release),
            Mode::Pinching { .. } if cancelled => self.emit(TouchAction::EndPinch { cancelled }),
            _ => {}
        }
        self.mode = Mode::Suppressed;
        self.dirty = false;
    }

    fn finish_empty(&mut self) {
        if self.contacts.is_empty() {
            self.device = None;
            self.mode = Mode::Pending;
            self.dirty = false;
        }
    }

    fn discard_pending_input(&mut self) {
        self.pending.retain(|(_, action)| {
            matches!(action, TouchAction::Release | TouchAction::EndPinch { .. })
        });
    }

    fn discard_pending_input_for(&mut self, device: &str) {
        self.pending.retain(|(owner, action)| {
            owner != device || matches!(action, TouchAction::Release | TouchAction::EndPinch { .. })
        });
    }

    fn emit(&mut self, action: TouchAction) {
        if let Some(device) = &self.device {
            self.pending.push((device.clone(), action));
        }
    }
}

fn finite(position: Position) -> bool {
    position.x.is_finite() && position.y.is_finite()
}

#[cfg(test)]
mod tests;
