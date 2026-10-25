//! Keep native app contacts on the original surface without widening CSD authority.
use super::*;

impl App {
    pub(super) fn begin_additional_ui_touch(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Position,
        time: InputTime,
    ) {
        self.maintain_ui_touch();
        let Some(touch) = self
            .ui_touch
            .as_ref()
            .filter(|touch| touch.device == device)
        else {
            // Hardware slots are device-local. The tracker also ignores a
            // second device until its owning sequence drains.
            return;
        };
        if touch.owns(device, slot) {
            // A repeated down must not increment Smithay's implicit grab count.
            return;
        }
        let Target::Client(client) = &touch.target else {
            self.cancel_ui_touch();
            return;
        };
        if !client.accepts_second_contact
            || client.slots.len() >= 2
            || !location.x.is_finite()
            || !location.y.is_finite()
            || !touch.output.contains(location.to_i32_round())
        {
            // Cancel before the tracker sees a third contact and can claim a
            // canvas gesture. Shell and layout routes still cancel on two.
            self.cancel_ui_touch();
            return;
        }
        let surface = client.surface.clone();
        // Smithay's TouchDownGrab retains the first surface for every slot.
        // Its original transform includes surface offsets, CSD insets and zoom;
        // re-hit-testing here would apply another window's coordinates to it.
        let local = touch.local_position(location);
        let window = touch.window;
        self.invalidate_gtk_touch_window(window);
        let Target::Client(client) = &mut self.ui_touch.as_mut().unwrap().target else {
            unreachable!();
        };
        client.slots.push(slot);
        // Never restore authority when either finger lifts or its slot is reused.
        // The original down's exact serial remains valid only for a single tap.
        client.single_contact_authority = false;
        self.touch.clone().down(
            self,
            Some((surface, (0.0, 0.0).into())),
            &DownEvent {
                slot,
                location: local,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
    }

    pub(super) fn end_client_ui_touch(&mut self, device: &str, slot: TouchSlot, time: InputTime) {
        let Some(touch) = self
            .ui_touch
            .as_ref()
            .filter(|touch| touch.owns(device, slot))
        else {
            return;
        };
        let Target::Client(client) = &touch.target else {
            return;
        };
        let serial = client.serial;
        let owned = self.ui_touch_has_client_grab(&client.surface, serial);
        self.touch.clone().up(
            self,
            &UpEvent {
                slot,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        self.record_gtk_touch_up(device, slot, serial, owned);
        let touch = self.ui_touch.as_mut().unwrap();
        let Target::Client(client) = &mut touch.target else {
            return;
        };
        client.slots.retain(|contact| *contact != slot);
        if client.slots.is_empty() {
            // Retain pointer suppression until the tracker flushes this frame.
            // Smithay retains the up target to frame the final motion and up.
            touch.target = Target::Cancelled;
        }
    }
}
