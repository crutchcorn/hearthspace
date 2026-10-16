//! Media keys never leak to clients; repeating level keys use compositor time.
use super::*;
use crate::compositor::shell_integration::device_control::DeviceControl;
use std::time::{Duration, Instant};

const REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_INTERVAL: Duration = Duration::from_millis(80);

pub(super) struct DeviceRepeat {
    code: Keycode,
    action: DeviceControl,
    next: Instant,
}

impl KeyboardShortcuts {
    pub(in crate::compositor) fn cancel_device_repeat(&mut self) {
        self.device_repeat = None;
    }

    pub(in crate::compositor) fn device_repeat_timeout(&self) -> Option<Duration> {
        self.device_repeat
            .as_ref()
            .map(|repeat| repeat.next.saturating_duration_since(Instant::now()))
    }

    pub(super) fn device_filter(
        &mut self,
        code: Keycode,
        state: KeyState,
        symbol: u32,
        available: bool,
    ) -> Option<FilterResult<Option<Shortcut>>> {
        let action = match symbol {
            keysyms::KEY_XF86AudioRaiseVolume => DeviceControl::VolumeUp,
            keysyms::KEY_XF86AudioLowerVolume => DeviceControl::VolumeDown,
            keysyms::KEY_XF86AudioMute => DeviceControl::VolumeMute,
            keysyms::KEY_XF86MonBrightnessUp => DeviceControl::BrightnessUp,
            keysyms::KEY_XF86MonBrightnessDown => DeviceControl::BrightnessDown,
            _ => return None,
        };
        if state == KeyState::Released {
            self.intercepted.remove(&code);
            if self
                .device_repeat
                .as_ref()
                .is_some_and(|repeat| repeat.code == code)
            {
                self.cancel_device_repeat();
            }
            return Some(FilterResult::Intercept(None));
        }
        // Ignore driver duplicate presses; only this bounded timer repeats.
        let first = self.intercepted.insert(code);
        if !available || !first {
            return Some(FilterResult::Intercept(None));
        }
        self.device_repeat = action.repeats().then_some(DeviceRepeat {
            code,
            action,
            next: Instant::now() + REPEAT_DELAY,
        });
        Some(FilterResult::Intercept(Some(Shortcut::DeviceControl(
            action,
        ))))
    }

    fn repeat_device_action(&mut self, now: Instant) -> Option<DeviceControl> {
        let repeat = self.device_repeat.as_mut()?;
        if now < repeat.next {
            return None;
        }
        // Never catch up queued repeats after a stalled event loop.
        repeat.next = now + REPEAT_INTERVAL;
        Some(repeat.action)
    }
}

impl App {
    pub(in crate::compositor) fn repeat_device_control(&mut self) {
        if let Some(action) = self.shortcuts.repeat_device_action(Instant::now())
            && let Err(error) = self.send_device_control(action)
        {
            tracing::debug!(%error, "repeated device control unavailable");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_keys_repeat_with_a_delay_but_mute_and_duplicate_presses_do_not() {
        let mut shortcuts = KeyboardShortcuts::default();
        let code = 123_u32.into();
        assert!(matches!(
            shortcuts.device_filter(
                code,
                KeyState::Pressed,
                keysyms::KEY_XF86AudioRaiseVolume,
                true
            ),
            Some(FilterResult::Intercept(Some(Shortcut::DeviceControl(
                DeviceControl::VolumeUp
            ))))
        ));
        assert!(shortcuts.repeat_device_action(Instant::now()).is_none());
        let next = shortcuts.device_repeat.as_ref().unwrap().next;
        assert_eq!(
            shortcuts.repeat_device_action(next),
            Some(DeviceControl::VolumeUp)
        );
        assert_eq!(shortcuts.repeat_device_action(next), None);
        assert!(matches!(
            shortcuts.device_filter(
                code,
                KeyState::Pressed,
                keysyms::KEY_XF86AudioRaiseVolume,
                true
            ),
            Some(FilterResult::Intercept(None))
        ));
        assert!(matches!(
            shortcuts.device_filter(
                code,
                KeyState::Released,
                keysyms::KEY_XF86AudioRaiseVolume,
                false
            ),
            Some(FilterResult::Intercept(None))
        ));
        assert!(shortcuts.device_repeat.is_none());
        shortcuts.device_filter(code, KeyState::Pressed, keysyms::KEY_XF86AudioMute, true);
        assert!(shortcuts.device_repeat.is_none());
        shortcuts.device_filter(code, KeyState::Released, keysyms::KEY_XF86AudioMute, true);
        shortcuts.device_filter(
            code,
            KeyState::Pressed,
            keysyms::KEY_XF86MonBrightnessUp,
            false,
        );
        assert!(shortcuts.device_repeat.is_none());
        shortcuts.device_filter(
            code,
            KeyState::Released,
            keysyms::KEY_XF86MonBrightnessUp,
            true,
        );
        assert!(shortcuts.intercepted.is_empty());
        assert!(
            shortcuts
                .device_filter(code, KeyState::Pressed, keysyms::KEY_a, true)
                .is_none()
        );
    }

    #[test]
    fn session_reset_cancels_repeats_and_large_delays_never_burst() {
        let mut shortcuts = KeyboardShortcuts::default();
        shortcuts.device_filter(
            123_u32.into(),
            KeyState::Pressed,
            keysyms::KEY_XF86MonBrightnessDown,
            true,
        );
        let late = Instant::now() + Duration::from_secs(20);
        assert_eq!(
            shortcuts.repeat_device_action(late),
            Some(DeviceControl::BrightnessDown)
        );
        assert_eq!(shortcuts.repeat_device_action(late), None);
        shortcuts.cancel_device_repeat();
        assert_eq!(shortcuts.repeat_device_action(late + REPEAT_INTERVAL), None);
    }
}
