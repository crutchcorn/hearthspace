//! Many-finger pinch/swipe intent is claimed once per contact chord.
use super::{Mode, Position, TouchAction, TouchTracker, finite};

impl TouchTracker {
    pub(super) fn recognize(&mut self) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        match self.mode {
            Mode::Pending | Mode::Dragging => self.single_motion(),
            Mode::Multiple if (3..=5).contains(&self.contacts.len()) => {
                if let Some((anchor, radius)) = self.geometry() {
                    if self.contacts.len() == 3 {
                        self.mode = Mode::Pinching { anchor, radius };
                        self.emit(TouchAction::BeginPinch { anchor });
                    } else {
                        self.mode = Mode::SwitcherPinch {
                            anchor,
                            radius,
                            pinching: false,
                        };
                        self.emit(TouchAction::BeginSwitcherPinch {
                            fingers: self.contacts.len() as u32,
                        });
                    }
                }
            }
            Mode::Pinching { anchor, radius } => {
                if let Some((centroid, current)) = self.geometry() {
                    let scale = current / radius;
                    let translation = centroid - anchor;
                    if scale.is_finite() && scale > 0.0 && finite(translation) {
                        self.emit(TouchAction::UpdatePinch { scale, translation });
                    }
                }
            }
            Mode::SwitcherPinch {
                anchor,
                radius,
                pinching,
            } => {
                self.recognize_many_fingers(anchor, radius, pinching);
            }
            Mode::Switching { anchor } => {
                if let Some((centroid, _)) = self.geometry() {
                    self.emit(TouchAction::UpdateSwitcher {
                        translation: centroid - anchor,
                    });
                }
            }
            _ => {}
        }
    }

    fn recognize_many_fingers(&mut self, anchor: Position, radius: f64, pinching: bool) {
        let Some((centroid, current)) = self.geometry() else {
            return;
        };
        let scale = current / radius;
        let translation = centroid - anchor;
        if !scale.is_finite() || scale <= 0.0 || !finite(translation) {
            return;
        }
        // Pinch wins when a frame contains meaningful spread and translation.
        // Once claimed, later translation or contraction cannot change intent.
        if pinching || scale.ln().abs() >= 0.08 {
            self.mode = Mode::SwitcherPinch {
                anchor,
                radius,
                pinching: true,
            };
            self.emit(TouchAction::UpdateSwitcherPinch { scale });
        } else if translation.x.abs() >= 32.0 && translation.x.abs() >= translation.y.abs() * 1.5 {
            self.emit(TouchAction::EndSwitcherPinch { cancelled: true });
            self.mode = Mode::Switching { anchor };
            self.emit(TouchAction::BeginSwitcher {
                fingers: self.contacts.len() as u32,
            });
            self.emit(TouchAction::UpdateSwitcher { translation });
        } else if translation.y.abs() >= 32.0 && translation.y.abs() > translation.x.abs() * 1.5 {
            self.emit(TouchAction::EndSwitcherPinch { cancelled: true });
            self.mode = Mode::Suppressed;
        } else {
            self.emit(TouchAction::UpdateSwitcherPinch { scale });
        }
    }
}
