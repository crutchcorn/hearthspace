use std::collections::HashSet;

use hearthspace_ipc::shortcuts::CanvasAction;
use smithay::{
    backend::input::{InputTime, KeyState},
    input::keyboard::{FilterResult, KeyboardSource, Keycode, ModifiersState, keysyms},
    utils::SERIAL_COUNTER,
};

use crate::compositor::{App, idle::ActivityReason, switcher::SwitcherSource};

mod actions;
mod canvas;
mod device_control;
mod groups;
mod meta;
mod switcher;

#[derive(Default)]
pub(in crate::compositor) struct KeyboardShortcuts {
    intercepted: HashSet<Keycode>,
    meta: meta::MetaTap,
    device_repeat: Option<device_control::DeviceRepeat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shortcut {
    CanvasSearch(i32),
    Canvas(CanvasAction),
    CanvasPanCancel,
    Desktop(crate::compositor::desktop::Action),
    Group(groups::Action),
    DeviceControl(crate::compositor::shell_integration::device_control::DeviceControl),
    Lock,
    SwitcherCancel,
    SwitcherCommit,
    ShellPanelDismiss,
    ShellCornerCycle { reverse: bool },
    Terminal,
    Launcher,
    Next,
    Previous,
    Close,
    Left,
    Right,
    Up,
    Down,
    Fullscreen,
    FocusedView,
    Maximize,
}

impl KeyboardShortcuts {
    pub(in crate::compositor) fn cancel_meta_tap(&mut self) {
        self.meta.cancel();
    }

    pub(in crate::compositor) fn discard_pending_key(
        &mut self,
        code: Keycode,
        state: KeyState,
        finish_existing: bool,
    ) -> FilterResult<()> {
        self.meta.discard(code, state);
        match state {
            KeyState::Pressed => {
                self.intercepted.insert(code);
            }
            KeyState::Released => {
                if !self.intercepted.remove(&code) && finish_existing {
                    return FilterResult::Forward;
                }
            }
        }
        FilterResult::Intercept(())
    }

    fn filter_shell_panel(
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
                    .then_some(Shortcut::ShellPanelDismiss),
            );
        }
        self.filter(code, state, modifiers, symbol, available)
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
    pub(in crate::compositor) fn handle_keyboard_input(
        &mut self,
        code: Keycode,
        state: KeyState,
        time: InputTime,
    ) {
        self.handle_keyboard_input_from_source(KeyboardSource::MAIN, code, state, time);
    }

    pub(in crate::compositor) fn handle_keyboard_input_from_source(
        &mut self,
        source: KeyboardSource,
        code: Keycode,
        state: KeyState,
        time: InputTime,
    ) {
        if !self.activate_keyboard_source(source)
            || self.buffer_pending_search_key(source, code, state, time)
        {
            return;
        }
        // Discarding pending search input can activate another source's map.
        // Restore this event's keymap and modifiers before normal filtering.
        if !self.activate_keyboard_source(source) {
            return;
        }
        let keyboard = self.keyboard.clone();
        let available = self.navigation_available();
        let canvas_available = self.canvas_shortcuts_available();
        let pan_escape = self.canvas_pan_captured();
        let mut toggle_launcher = false;
        let mut layout_escape = false;
        let mut power_escape = false;
        let shell_panel_focused = self.active_shell_panel_index().is_some()
            || self.focused_shell_corner_index().is_some();
        let group_visible = self.group_keyboard_visible();
        let previous_modifiers = keyboard.modifier_state();
        let intercepted = keyboard.input_from_source(
            source,
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
                toggle_launcher = app
                    .shortcuts
                    .meta
                    .observe(code, state, modifiers, symbol, available);
                if pan_escape && state == KeyState::Pressed && symbol == keysyms::KEY_Escape {
                    FilterResult::Intercept(
                        app.shortcuts
                            .intercepted
                            .insert(code)
                            .then_some(Shortcut::CanvasPanCancel),
                    )
                } else if app.power_confirmation_active() {
                    if state == KeyState::Pressed && symbol == keysyms::KEY_Escape {
                        power_escape = app.shortcuts.intercepted.insert(code);
                        FilterResult::Intercept(None)
                    } else {
                        app.shortcuts.filter(code, state, modifiers, symbol, false)
                    }
                } else if let Some(result) =
                    app.shortcuts
                        .device_filter(code, state, symbol, !app.session_locked())
                {
                    result
                } else if canvas_available
                    && state == KeyState::Pressed
                    && let Some(action) = canvas::shortcut(modifiers, symbol)
                {
                    FilterResult::Intercept(
                        app.shortcuts
                            .intercepted
                            .insert(code)
                            .then_some(Shortcut::Canvas(action)),
                    )
                } else if (app.layout_editing() || app.layout_confirmation_active())
                    && state == KeyState::Pressed
                    && symbol == 0xff1b
                {
                    layout_escape = app.shortcuts.intercepted.insert(code);
                    FilterResult::Intercept(None)
                } else if state == KeyState::Pressed
                    && let Some(action) =
                        app.canvas_search_key_action(modifiers, symbol, shell_panel_focused)
                {
                    FilterResult::Intercept(
                        app.shortcuts
                            .intercepted
                            .insert(code)
                            .then_some(Shortcut::CanvasSearch(action)),
                    )
                } else if state == KeyState::Pressed
                    && app.canvas_search_active()
                    && let Some(action @ (Shortcut::Next | Shortcut::Previous)) =
                        shortcut(modifiers, symbol)
                {
                    FilterResult::Intercept(
                        app.shortcuts.intercepted.insert(code).then_some(action),
                    )
                } else if let Some(source) = app.window_switcher_source() {
                    app.shortcuts
                        .filter_switcher(code, state, modifiers, symbol, source)
                } else if shell_panel_focused {
                    app.shortcuts
                        .filter_shell_panel(code, state, modifiers, symbol, available)
                } else if group_visible {
                    let selecting = app.group_selection_active();
                    app.shortcuts
                        .filter_group(code, state, modifiers, symbol, available, selecting)
                } else if let Some(action) = app.desktop_key_action(state, modifiers, symbol) {
                    FilterResult::Intercept(
                        app.shortcuts
                            .intercepted
                            .insert(code)
                            .then_some(Shortcut::Desktop(action)),
                    )
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
        if keyboard.modifier_state().alt != previous_modifiers.alt {
            self.refresh_grid_snapping();
        }
        if layout_escape {
            self.layout_escape();
        }
        if power_escape {
            self.dismiss_power_confirmation();
        }
        match intercepted {
            Some(Some(shortcut)) => self.run_shortcut(shortcut),
            Some(None) => {}
            None => self.record_focused_client_activity(ActivityReason::ClientInput),
        }
        if toggle_launcher && let Err(error) = self.toggle_launcher() {
            tracing::debug!(%error, "search toggle unavailable");
        }
        // Observe XKB's aggregate Alt state, including either physical Alt key.
        // Releasing Shift, Tab, or one of two held Alt keys must keep the overlay.
        if previous_modifiers.alt && !keyboard.modifier_state().alt {
            self.finish_window_switcher(SwitcherSource::Keyboard, false);
        }
        self.refresh_canvas_pan_modifiers(&previous_modifiers, time);
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
    if modifiers.logo && !modifiers.alt && symbol == keysyms::KEY_F6 {
        return Some(Shortcut::ShellCornerCycle {
            reverse: modifiers.shift,
        });
    }
    if modifiers.logo
        && !modifiers.alt
        && modifiers.shift
        && matches!(symbol, keysyms::KEY_f | keysyms::KEY_F)
    {
        return Some(Shortcut::FocusedView);
    }
    if !modifiers.logo || modifiers.alt || modifiers.shift {
        return None;
    }
    match symbol {
        keysyms::KEY_l => Some(Shortcut::Lock),
        keysyms::KEY_Return => Some(Shortcut::Terminal),
        keysyms::KEY_space => Some(Shortcut::Launcher),
        keysyms::KEY_Left => Some(Shortcut::Left),
        keysyms::KEY_Right => Some(Shortcut::Right),
        keysyms::KEY_Up => Some(Shortcut::Up),
        keysyms::KEY_Down => Some(Shortcut::Down),
        keysyms::KEY_f => Some(Shortcut::Fullscreen),
        keysyms::KEY_m => Some(Shortcut::Maximize),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
