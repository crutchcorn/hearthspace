use std::time::{Duration, Instant};

use smithay::{
    backend::input::{ButtonState, TouchSlot},
    input::{
        Seat,
        pointer::{ButtonEvent, ClickGrab},
    },
    utils::Serial,
};
use wayland_server::{
    Resource,
    protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
};

use super::protocol::gtk_surface1::Gesture;
use crate::compositor::{App, WindowDecoration};

const PRESS_LIFETIME: Duration = Duration::from_secs(1);

fn seat_matches<D: smithay::input::SeatHandler>(resource: &WlSeat, expected: &Seat<D>) -> bool {
    Seat::<D>::from_resource(resource).as_ref() == Some(expected)
}

enum PressSource {
    Pointer(u32),
    Touch {
        device: String,
        slot: TouchSlot,
        window: u64,
    },
}

pub(super) struct TitlebarPress {
    serial: Serial,
    source: PressSource,
    surface: WlSurface,
    received: Instant,
    released: bool,
}

impl App {
    /// Snapshot ownership before Smithay removes the grab on button release.
    pub(in crate::compositor) fn gtk_pointer_release_owned(&self, button: u32) -> bool {
        let Some(press) = self.gtk_shell.press.as_ref().filter(
            |press| matches!(press.source, PressSource::Pointer(pressed) if pressed == button),
        ) else {
            return false;
        };
        self.pointer
            .with_grab(|serial, grab| {
                serial == press.serial
                    && grab.is::<ClickGrab<Self>>()
                    && grab.start_data().button == button
                    && grab
                        .start_data()
                        .focus
                        .as_ref()
                        .is_some_and(|(surface, _)| *surface == press.surface)
            })
            .unwrap_or(false)
    }

    /// Record only presses actually delivered through Smithay's implicit grab.
    /// A completed click or emulated tap may precede the client's round trip.
    pub(in crate::compositor) fn record_gtk_pointer_button(
        &mut self,
        event: &ButtonEvent,
        owned_release: bool,
    ) {
        if event.state == ButtonState::Released {
            if let Some(press) = self.gtk_shell.press.as_mut()
                && matches!(press.source, PressSource::Pointer(button) if button == event.button)
            {
                if owned_release {
                    press.released = true;
                } else {
                    self.gtk_shell.reset_input();
                }
            }
            return;
        }
        self.gtk_shell.press = self
            .pointer
            .with_grab(|serial, grab| {
                if serial != event.serial || !grab.is::<ClickGrab<Self>>() {
                    return None;
                }
                let (surface, _) = grab.start_data().focus.as_ref()?;
                Some(TitlebarPress {
                    serial,
                    source: PressSource::Pointer(event.button),
                    surface: surface.clone(),
                    received: Instant::now(),
                    released: false,
                })
            })
            .flatten();
    }

    pub(in crate::compositor) fn record_gtk_touch_down(
        &mut self,
        device: &str,
        slot: TouchSlot,
        surface: &WlSurface,
        serial: Serial,
        window: u64,
    ) {
        self.gtk_shell.press =
            self.ui_touch_has_client_grab(surface, serial)
                .then(|| TitlebarPress {
                    serial,
                    source: PressSource::Touch {
                        device: device.into(),
                        slot,
                        window,
                    },
                    surface: surface.clone(),
                    received: Instant::now(),
                    released: false,
                });
    }

    pub(in crate::compositor) fn record_gtk_touch_up(
        &mut self,
        device: &str,
        slot: TouchSlot,
        serial: Serial,
        owned: bool,
    ) {
        let Some(press) = self.gtk_shell.press.as_mut() else {
            return;
        };
        if press.serial == serial
            && matches!(&press.source,
            PressSource::Touch { device: owner, slot: contact, .. } if owner == device && *contact == slot)
        {
            if owned {
                press.released = true;
            } else {
                self.gtk_shell.reset_input();
            }
        }
    }

    pub(in crate::compositor) fn cancel_gtk_touch(&mut self, device: &str, slot: TouchSlot) {
        // A completed tap can outlive UiTouch while GTK's request is in flight.
        // Cancellation must revoke that token even after the last frame drained.
        if self.gtk_shell.press.as_ref().is_some_and(|press| {
            matches!(&press.source,
            PressSource::Touch { device: owner, slot: contact, .. }
                if owner == device && (i32::from(slot) < 0 || *contact == slot))
        }) {
            self.gtk_shell.reset_input();
        }
    }

    pub(in crate::compositor) fn invalidate_gtk_touch_window(&mut self, id: u64) {
        if self.gtk_shell.press.as_ref().is_some_and(|press| {
            matches!(
                press.source, PressSource::Touch { window, .. } if window == id
            )
        }) {
            self.gtk_shell.reset_input();
        }
    }

    pub(in crate::compositor) fn gtk_touch_focus_changed(&mut self, surface: Option<&WlSurface>) {
        if self.gtk_shell.press.as_ref().is_some_and(|press| {
            matches!(press.source, PressSource::Touch { .. }) && Some(&press.surface) != surface
        }) {
            self.gtk_shell.reset_input();
        }
    }

    pub(super) fn gtk_titlebar_gesture(
        &mut self,
        surface: &WlSurface,
        seat: &WlSeat,
        serial: Serial,
        gesture: Gesture,
    ) {
        // Right/middle currently have the same no-action policy as xdg-shell's
        // show_window_menu/minimize requests. See protocols/README.md.
        if gesture != Gesture::DoubleClick {
            return;
        }
        let Some(press) = self.gtk_shell.press.as_ref() else {
            return;
        };
        if !seat_matches(seat, &self._seat)
            || press.serial != serial
            || matches!(press.source, PressSource::Pointer(button) if button != 0x110)
            || press.surface != *surface
            || !surface.is_alive()
        {
            return;
        }
        // Consume before applying policy: another gtk_surface wrapper or a
        // later retry after an overlay closes must not reuse the same input.
        let press = self.gtk_shell.press.take().unwrap();
        if press.received.elapsed() > PRESS_LIFETIME {
            return;
        }
        let (own_grab, completed_tap) = match &press.source {
            PressSource::Pointer(button) => {
                let own = self
                    .pointer
                    .with_grab(|grab_serial, grab| {
                        grab_serial == serial
                            && grab.is::<ClickGrab<Self>>()
                            && grab.start_data().button == *button
                            && grab
                                .start_data()
                                .focus
                                .as_ref()
                                .is_some_and(|(target, _)| target == surface)
                    })
                    .unwrap_or(false);
                let completed = press.released
                    && !self.pointer.is_grabbed()
                    && self.pointer.current_focus().as_ref() == Some(surface);
                (own, completed)
            }
            PressSource::Touch { .. } => {
                // Touch never moves pointer focus. Use the actual keyboard
                // surface, not merely another surface of the same client/window.
                let focused = self
                    .keyboard
                    .current_focus()
                    .is_some_and(|focus| focus.surface() == surface);
                let available = focused && !self.pointer.is_grabbed();
                (
                    available && self.ui_touch_has_client_grab(surface, serial),
                    available && press.released && !self.touch.is_grabbed(),
                )
            }
        };
        if !(own_grab || completed_tap)
            || self.session_locked()
            || self.canvas_search_active()
            || self.window_switcher_active()
            || self.layout_editing()
            || self.modal_confirmation_active()
            || self.keyboard.is_grabbed()
            || self.drag.is_some()
            || self.resize.is_some()
            || self.group_interaction_active()
        {
            return;
        }
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.surface.wl_surface().as_ref() == Some(surface))
        else {
            return;
        };
        if !self.is_navigable_window(index)
            || self.windows[index].decoration != WindowDecoration::ClientSide
            || self.focused_normal_window_id != Some(self.windows[index].id)
        {
            return;
        }
        let id = self.windows[index].id;
        if let Err(error) = self.toggle_window_canvas_maximize_from_titlebar(id) {
            tracing::debug!(window_id = id, %error, "GTK titlebar gesture unavailable");
        }
    }
}

#[cfg(test)]
#[path = "seat_tests.rs"]
mod seat_tests;
