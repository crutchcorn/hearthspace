use std::time::Instant;

use smithay::{
    backend::input::TouchSlot,
    utils::{Logical, Point},
};

use super::App;

impl App {
    pub(in crate::compositor) fn favorite_touch_down(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Point<f64, Logical>,
    ) -> bool {
        self.maintain_favorite_drawing();
        if self.touch_input.has_contacts() {
            return false;
        }
        let point = self.favorite_local(location);
        let owned = self
            .favorite_drawing
            .touch_down(device, slot, point, Instant::now());
        self.flush_favorite_events();
        owned
    }

    pub(in crate::compositor) fn favorite_touch_motion(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Point<f64, Logical>,
    ) -> bool {
        self.maintain_favorite_drawing();
        let point = self.favorite_local(location);
        let owned = self
            .favorite_drawing
            .touch_motion(device, slot, point, Instant::now());
        self.flush_favorite_events();
        owned
    }

    pub(in crate::compositor) fn favorite_touch_up(
        &mut self,
        device: &str,
        slot: TouchSlot,
        cancelled: bool,
    ) -> bool {
        self.maintain_favorite_drawing();
        let owned = self
            .favorite_drawing
            .touch_up(device, slot, cancelled, Instant::now());
        self.flush_favorite_events();
        owned
    }

    pub(in crate::compositor) fn favorite_device_removed(&mut self, device: &str) {
        self.favorite_drawing.touch_removed(device);
        self.flush_favorite_events();
    }

    pub(in crate::compositor) fn reset_favorite_input(&mut self) {
        self.clear_favorite_drawing();
        self.favorite_drawing.pointer_held = false;
        self.favorite_drawing.contacts.clear();
    }
}
