use smithay::input::keyboard::keysyms;
use tracing::debug;

use crate::{compositor::App, config::SCROLL_ZOOM_SENSITIVITY};

impl App {
    /// Both physical and control-socket input enter the same policy after wheel
    /// steps have been converted into logical scroll pixels.
    pub(super) fn zoom_from_scroll_amount(&mut self, vertical: Option<f64>) -> bool {
        self.shortcuts.cancel_meta_tap();
        if self.canvas_search_active() || self.layout_confirmation_active() {
            return self.hit_test(self.pointer_location).is_none();
        }
        if self.overview_active()
            || self.window_switcher_active()
            || self.group_interaction_active()
        {
            return true;
        }
        if self.session_locked() {
            return false;
        }
        let super_active = self.keyboard.modifier_state().logo
            || self.keyboard.with_pressed_keysyms(|pressed| {
                pressed
                    .iter()
                    .any(|key| is_super_keysym(key.modified_sym().raw()))
            });
        let Some(factor) =
            scroll_zoom_factor(self.scroll_zooms_without_super, super_active, vertical)
        else {
            return false;
        };
        if factor != 1.0 {
            self.advance_viewport_animation();
            debug!(?vertical, "zooming viewport from scroll input");
            self.animate_zoom_around_viewport_center(factor);
        }
        true
    }
}

fn is_super_keysym(keysym: u32) -> bool {
    matches!(
        keysym,
        keysyms::KEY_Super_L
            | keysyms::KEY_Super_R
            | keysyms::KEY_Meta_L
            | keysyms::KEY_Meta_R
            | keysyms::KEY_Hyper_L
            | keysyms::KEY_Hyper_R
    )
}

fn scroll_zoom_factor(
    without_super: bool,
    super_active: bool,
    vertical: Option<f64>,
) -> Option<f64> {
    if !without_super && !super_active {
        return None;
    }
    vertical
        .filter(|amount| amount.is_finite())
        .map(|amount| (-amount * SCROLL_ZOOM_SENSITIVITY).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_policy_respects_modifier_and_unmodified_zoom_setting() {
        for (setting, modifier, zooms) in [
            (false, false, false),
            (false, true, true),
            (true, false, true),
            (true, true, true),
        ] {
            assert_eq!(
                scroll_zoom_factor(setting, modifier, Some(-120.0)).is_some(),
                zooms
            );
        }
    }

    #[test]
    fn scroll_stop_is_consumed_without_zoom_and_absent_axis_is_forwarded() {
        assert_eq!(scroll_zoom_factor(false, true, Some(0.0)), Some(1.0));
        assert_eq!(scroll_zoom_factor(true, false, None), None);
        assert_eq!(scroll_zoom_factor(true, false, Some(f64::NAN)), None);
    }

    #[test]
    fn equal_logical_scroll_amounts_have_reciprocal_zoom_factors() {
        let zoom_in = scroll_zoom_factor(false, true, Some(-15.0)).unwrap();
        let zoom_out = scroll_zoom_factor(true, false, Some(15.0)).unwrap();
        assert!(zoom_in > 1.0);
        assert!(zoom_out < 1.0);
        assert!((zoom_in * zoom_out - 1.0).abs() < 1e-10);
    }

    #[test]
    fn super_meta_and_hyper_keys_enable_the_same_policy() {
        for key in [
            keysyms::KEY_Super_L,
            keysyms::KEY_Super_R,
            keysyms::KEY_Meta_L,
            keysyms::KEY_Meta_R,
            keysyms::KEY_Hyper_L,
            keysyms::KEY_Hyper_R,
        ] {
            assert!(is_super_keysym(key));
        }
        assert!(!is_super_keysym(keysyms::KEY_Shift_L));
    }
}
