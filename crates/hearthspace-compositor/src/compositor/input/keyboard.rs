use std::collections::HashSet;

use hearthspace_shell::{ShellCommand, SpawnTarget};
use smithay::{
    backend::input::{InputTime, KeyState},
    input::keyboard::{FilterResult, Keycode, ModifiersState, keysyms},
    utils::SERIAL_COUNTER,
};

use crate::compositor::{
    App, idle::ActivityReason, shell_integration::ControlReply, switcher::SwitcherSource,
};

mod switcher;

#[derive(Default)]
pub(in crate::compositor) struct KeyboardShortcuts {
    intercepted: HashSet<Keycode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shortcut {
    Lock,
    OverviewToggle,
    OverviewDismiss,
    OverviewActivate,
    OverviewNext,
    OverviewPrevious,
    SwitcherCancel,
    CanvasMapDismiss,
    Terminal,
    Launcher,
    Next,
    Previous,
    Close,
    Left,
    Right,
    Up,
    Down,
    Reset,
    Fullscreen,
    Maximize,
}

impl KeyboardShortcuts {
    fn filter_canvas_map(
        &mut self,
        code: Keycode,
        state: KeyState,
        modifiers: &ModifiersState,
        symbol: u32,
        available: bool,
    ) -> FilterResult<Option<Shortcut>> {
        if available && symbol == keysyms::KEY_Escape && state == KeyState::Pressed {
            return FilterResult::Intercept(
                self.intercepted
                    .insert(code)
                    .then_some(Shortcut::CanvasMapDismiss),
            );
        }
        self.filter(code, state, modifiers, symbol, available)
    }

    fn filter_overview(
        &mut self,
        code: Keycode,
        state: KeyState,
        modifiers: &ModifiersState,
        symbol: u32,
    ) -> FilterResult<Option<Shortcut>> {
        if state == KeyState::Released || self.intercepted.contains(&code) {
            return self.filter(code, state, modifiers, symbol, false);
        }
        self.intercepted.insert(code);
        FilterResult::Intercept(overview_shortcut(modifiers, symbol))
    }

    fn filter(
        &mut self,
        code: Keycode,
        state: KeyState,
        modifiers: &ModifiersState,
        symbol: u32,
        available: bool,
    ) -> FilterResult<Option<Shortcut>> {
        if state == KeyState::Released {
            return if self.intercepted.remove(&code) {
                FilterResult::Intercept(None)
            } else {
                FilterResult::Forward
            };
        }
        if self.intercepted.contains(&code) {
            return FilterResult::Intercept(None);
        }
        let action = available.then(|| shortcut(modifiers, symbol)).flatten();
        if let Some(action) = action {
            self.intercepted.insert(code);
            FilterResult::Intercept(Some(action))
        } else {
            FilterResult::Forward
        }
    }
}

impl App {
    /// Real and agent keyboard input use the same policy. Intercepted releases
    /// stay intercepted even if the modifier was released first.
    pub(super) fn handle_keyboard_input(
        &mut self,
        code: Keycode,
        state: KeyState,
        time: InputTime,
    ) {
        let keyboard = self.keyboard.clone();
        let available = self.navigation_available();
        let canvas_map_focused = self.focused_canvas_map_index().is_some();
        let previous_modifiers = keyboard.modifier_state();
        let intercepted = keyboard.input(
            self,
            code,
            state,
            SERIAL_COUNTER.next_serial(),
            time,
            |app, modifiers, key| {
                let symbol = key
                    .raw_latin_sym_or_raw_current_sym()
                    .map(|symbol| symbol.raw())
                    .unwrap_or(0);
                if app.overview_active() {
                    app.shortcuts
                        .filter_overview(code, state, modifiers, symbol)
                } else if let Some(source) = app.window_switcher_source() {
                    app.shortcuts
                        .filter_switcher(code, state, modifiers, symbol, source)
                } else if canvas_map_focused {
                    app.shortcuts
                        .filter_canvas_map(code, state, modifiers, symbol, available)
                } else {
                    app.shortcuts
                        .filter(code, state, modifiers, symbol, available)
                }
            },
        );
        // A modifier pressed in a modal overlay can remain held across dismissal.
        // Its intercepted release must still clear the state sent on client
        // keyboard enter, without forwarding an unmatched key release.
        if intercepted.is_some() && keyboard.modifier_state() != previous_modifiers {
            keyboard.advertise_modifier_state(self);
        }
        match intercepted {
            Some(Some(shortcut)) => self.run_shortcut(shortcut),
            Some(None) => {}
            None => self.record_focused_client_activity(ActivityReason::ClientInput),
        }
        // Observe XKB's aggregate Alt state, including either physical Alt key.
        // Releasing Shift, Tab, or one of two held Alt keys must keep the overlay.
        if previous_modifiers.alt && !keyboard.modifier_state().alt {
            self.finish_window_switcher(SwitcherSource::Keyboard, false);
        }
    }

    fn run_shortcut(&mut self, shortcut: Shortcut) {
        match shortcut {
            Shortcut::Next | Shortcut::Previous => {
                if let Err(error) = self
                    .cycle_window_switcher(SwitcherSource::Keyboard, shortcut == Shortcut::Previous)
                {
                    tracing::debug!(%error, "window switching unavailable");
                }
                return;
            }
            Shortcut::SwitcherCancel => {
                self.dismiss_window_switcher(true);
                return;
            }
            Shortcut::CanvasMapDismiss => {
                self.dismiss_focused_canvas_map();
                return;
            }
            Shortcut::OverviewDismiss => {
                self.close_overview();
                return;
            }
            Shortcut::OverviewActivate => {
                if let Err(error) = self.overview_activate_selected() {
                    tracing::debug!(%error, "overview selection unavailable");
                }
                return;
            }
            Shortcut::OverviewNext | Shortcut::OverviewPrevious => {
                self.overview_selection_cycle(shortcut == Shortcut::OverviewNext);
                return;
            }
            _ => {}
        }
        let command = match shortcut {
            Shortcut::OverviewToggle => ShellCommand::OverviewToggle,
            Shortcut::OverviewDismiss
            | Shortcut::OverviewActivate
            | Shortcut::OverviewNext
            | Shortcut::OverviewPrevious
            | Shortcut::SwitcherCancel
            | Shortcut::CanvasMapDismiss
            | Shortcut::Next
            | Shortcut::Previous => unreachable!(),
            Shortcut::Lock => ShellCommand::LockSession,
            Shortcut::Terminal => ShellCommand::Spawn(SpawnTarget::Foot),
            Shortcut::Launcher => ShellCommand::FocusLauncher,
            Shortcut::Close => {
                let Some(id) = self.focused_normal_window_id else {
                    return;
                };
                ShellCommand::CloseWindow(id)
            }
            Shortcut::Left => ShellCommand::PanLeft,
            Shortcut::Right => ShellCommand::PanRight,
            Shortcut::Up => ShellCommand::PanUp,
            Shortcut::Down => ShellCommand::PanDown,
            Shortcut::Reset => ShellCommand::ResetView,
            Shortcut::Fullscreen | Shortcut::Maximize => {
                let Some(id) = self.focused_normal_window_id else {
                    return;
                };
                let Some(window) = self.windows.iter().find(|window| window.id == id) else {
                    return;
                };
                if shortcut == Shortcut::Fullscreen {
                    ShellCommand::FullscreenWindow {
                        id,
                        enabled: !window.presentation.fullscreen,
                    }
                } else {
                    ShellCommand::MaximizeWindow {
                        id,
                        enabled: !window.presentation.maximized,
                    }
                }
            }
        };
        if let ControlReply::Err(error) = self.run_control_action(command) {
            tracing::warn!(%error, ?shortcut, "keyboard action failed");
        }
    }
}

fn overview_shortcut(modifiers: &ModifiersState, symbol: u32) -> Option<Shortcut> {
    if matches!(
        shortcut(modifiers, symbol),
        Some(Shortcut::Lock | Shortcut::OverviewToggle)
    ) {
        return shortcut(modifiers, symbol);
    }
    if modifiers.ctrl || modifiers.alt || modifiers.logo {
        return None;
    }
    match symbol {
        keysyms::KEY_Escape => Some(Shortcut::OverviewDismiss),
        keysyms::KEY_Return | keysyms::KEY_KP_Enter => Some(Shortcut::OverviewActivate),
        keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab => Some(if modifiers.shift {
            Shortcut::OverviewPrevious
        } else {
            Shortcut::OverviewNext
        }),
        keysyms::KEY_Left | keysyms::KEY_Up => Some(Shortcut::OverviewPrevious),
        keysyms::KEY_Right | keysyms::KEY_Down => Some(Shortcut::OverviewNext),
        _ => None,
    }
}

fn shortcut(modifiers: &ModifiersState, symbol: u32) -> Option<Shortcut> {
    if modifiers.ctrl {
        return None;
    }
    if modifiers.alt && !modifiers.logo {
        return match symbol {
            keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab => Some(if modifiers.shift {
                Shortcut::Previous
            } else {
                Shortcut::Next
            }),
            keysyms::KEY_F4 if !modifiers.shift => Some(Shortcut::Close),
            _ => None,
        };
    }
    if !modifiers.logo || modifiers.alt || modifiers.shift {
        return None;
    }
    match symbol {
        keysyms::KEY_o => Some(Shortcut::OverviewToggle),
        keysyms::KEY_l => Some(Shortcut::Lock),
        keysyms::KEY_Return => Some(Shortcut::Terminal),
        keysyms::KEY_space => Some(Shortcut::Launcher),
        keysyms::KEY_Left => Some(Shortcut::Left),
        keysyms::KEY_Right => Some(Shortcut::Right),
        keysyms::KEY_Up => Some(Shortcut::Up),
        keysyms::KEY_Down => Some(Shortcut::Down),
        keysyms::KEY_0 => Some(Shortcut::Reset),
        keysyms::KEY_f => Some(Shortcut::Fullscreen),
        keysyms::KEY_m => Some(Shortcut::Maximize),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
