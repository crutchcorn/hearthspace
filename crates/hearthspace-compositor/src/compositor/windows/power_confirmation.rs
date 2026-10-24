//! Native power dialogs share modal input policy without opening a layout session.
use hearthspace_common::power_confirm_output_name_from_app_id;
use smithay::{
    backend::input::InputTime,
    utils::{Logical, Point, Rectangle, Size},
};

use super::window_app_id;
use crate::compositor::App;

pub(in crate::compositor) struct PowerConfirmation {
    window: u64,
    previous_focus: Option<u64>,
}

impl App {
    pub(in crate::compositor) fn is_power_confirmation(&self, index: usize) -> bool {
        window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(power_confirm_output_name_from_app_id)
            .is_some()
    }

    pub(in crate::compositor) fn power_confirmation_active(&self) -> bool {
        !self.session_locked()
            && self.windows.iter().enumerate().any(|(index, window)| {
                window.mapped
                    && self.power_confirmation_surface(index)
                    && self
                        .output_logical_rect_for_name(window.shell_output_name.as_deref())
                        .is_some()
            })
    }

    pub(in crate::compositor) fn power_confirmation_surface(&self, index: usize) -> bool {
        self.power_confirmation
            .as_ref()
            .is_some_and(|dialog| dialog.window == self.windows[index].id)
    }

    pub(in crate::compositor) fn modal_confirmation_active(&self) -> bool {
        self.power_confirmation_active() || self.layout_confirmation_active()
    }

    pub(in crate::compositor) fn modal_confirmation_surface(&self, index: usize) -> bool {
        if self.power_confirmation_active() {
            self.power_confirmation_surface(index)
        } else {
            self.layout_confirmation_surface(index)
        }
    }

    pub(in crate::compositor) fn power_confirmation_on_output(
        &self,
        output: Rectangle<i32, Logical>,
    ) -> bool {
        self.power_confirmation_active()
            && self.windows.iter().enumerate().any(|(index, window)| {
                self.power_confirmation_surface(index)
                    && self.output_logical_rect_for_name(window.shell_output_name.as_deref())
                        == Some(output)
            })
    }

    pub(in crate::compositor) fn power_confirmation_origin(
        &self,
        index: usize,
    ) -> Option<Point<i32, Logical>> {
        let output =
            self.output_logical_rect_for_name(self.windows[index].shell_output_name.as_deref())?;
        let size = self.window_geometry_size(index);
        Some(
            output.loc
                + Point::from((
                    (output.size.w - size.w).max(0) / 2,
                    (output.size.h - size.h).max(0) / 2,
                ))
                - self.window_surface_geometry_loc(index),
        )
    }

    pub(in crate::compositor) fn power_confirmation_mapped(&mut self, index: usize) {
        // Check availability before accepting the dialog. Its own modal state
        // must not reject this initial mapping or replace an existing owner.
        if !self.navigation_available() || self.power_confirmation_origin(index).is_none() {
            self.windows[index].surface.send_close();
            return;
        }
        let previous_focus = self
            .keyboard
            .current_focus()
            .and_then(|focus| self.window_index_for_surface(focus.surface()))
            .filter(|index| self.is_navigable_window(*index))
            .or_else(|| {
                self.windows
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(index, _)| self.is_navigable_window(*index))
                    .map(|(index, _)| index)
            })
            .map(|index| self.windows[index].id);
        self.power_confirmation = Some(PowerConfirmation {
            window: self.windows[index].id,
            previous_focus,
        });
        self.cancel_ui_touch();
        self.clear_favorite_drawing();
        self.shortcuts.cancel_meta_tap();
        self.shortcuts.cancel_device_repeat();
        self.interrupt_pinch_zoom();
        if let Some(surface) = self.windows[index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(index, surface);
        }
        self.apply_pointer_motion(self.pointer_location, InputTime::now());
        self.request_redraw();
    }

    pub(in crate::compositor) fn dismiss_power_confirmation(&mut self) {
        if let Some(index) =
            (0..self.windows.len()).find(|&index| self.power_confirmation_surface(index))
        {
            // Closing is only cancellation. The shell owns explicit power
            // actions; neither Escape nor a compositor focus change sends one.
            self.cancel_ui_touch();
            self.windows[index].surface.send_close();
        }
    }

    pub(in crate::compositor) fn reset_power_confirmation(&mut self) {
        self.dismiss_power_confirmation();
        self.power_confirmation = None;
    }

    pub(in crate::compositor) fn finish_power_confirmation(&mut self, id: u64) {
        if !self
            .power_confirmation
            .as_ref()
            .is_some_and(|dialog| dialog.window == id)
        {
            return;
        }
        let dialog = self.power_confirmation.take().unwrap();
        if self.session_locked() {
            return;
        }
        let previous = dialog.previous_focus.and_then(|id| {
            self.windows
                .iter()
                .enumerate()
                .find(|(index, window)| window.id == id && self.is_navigable_window(*index))
                .map(|(index, _)| index)
        });
        let target = previous.or_else(|| {
            self.windows
                .iter()
                .enumerate()
                .rev()
                .find(|(index, _)| self.is_navigable_window(*index))
                .map(|(index, _)| index)
        });
        // An implicit pointer grab may still owe its release after Escape.
        // Restore only keyboard focus; retain that grab's original owner.
        if let Some(index) = target
            && let Some(surface) = self.windows[index].surface.wl_surface()
        {
            self.set_keyboard_focus_to_window(index, surface);
        }
        self.apply_pointer_motion(self.pointer_location, InputTime::now());
        self.request_redraw();
    }
}

pub(super) fn power_confirmation_size(
    requested: Size<i32, Logical>,
    output: Size<i32, Logical>,
) -> Size<i32, Logical> {
    (
        420.min(output.w.saturating_sub(48).max(1)),
        requested.h.min(output.h.saturating_sub(48).max(1)),
    )
        .into()
}
