//! Consume every contact belonging to a drawing chord, including after cancel.
use std::time::Instant;

use smithay::backend::input::TouchSlot;

use super::model::{FavoriteDrawing, Source};

impl FavoriteDrawing {
    pub(super) fn touch_down(
        &mut self,
        device: &str,
        slot: TouchSlot,
        point: Option<[f64; 2]>,
        now: Instant,
    ) -> bool {
        if !self.contacts.is_empty() {
            if self.contacts.len() < 64 {
                self.contacts.insert((device.to_owned(), slot));
            }
            self.cancel();
            return true;
        }
        let Some(point) = point else {
            return false;
        };
        self.contacts.insert((device.to_owned(), slot));
        if self.pointer_held {
            self.cancel();
        } else {
            self.begin(Source::Touch(device.to_owned(), slot), point, now);
        }
        true
    }

    pub(super) fn touch_motion(
        &mut self,
        device: &str,
        slot: TouchSlot,
        point: Option<[f64; 2]>,
        now: Instant,
    ) -> bool {
        if !self.contacts.contains(&(device.to_owned(), slot)) {
            return false;
        }
        if let Some(point) = point {
            self.update(&Source::Touch(device.to_owned(), slot), point, now);
        } else {
            self.cancel();
        }
        true
    }

    pub(super) fn touch_up(
        &mut self,
        device: &str,
        slot: TouchSlot,
        cancelled: bool,
        now: Instant,
    ) -> bool {
        if cancelled && i32::from(slot) < 0 {
            return self.touch_removed(device);
        }
        if !self.contacts.remove(&(device.to_owned(), slot)) {
            return false;
        }
        self.end(&Source::Touch(device.to_owned(), slot), cancelled, now);
        true
    }

    pub(super) fn touch_removed(&mut self, device: &str) -> bool {
        let owned = self.contacts.iter().any(|(owner, _)| owner == device);
        if owned {
            self.cancel();
            self.contacts.retain(|(owner, _)| owner != device);
        }
        owned
    }
}

#[cfg(test)]
#[path = "contact_tests.rs"]
mod tests;
