//! A released key can activate only the row that owned its original press.
use super::{ButtonAction, TrayButton};
use xilem::masonry::core::{
    EventCtx, TextEvent,
    keyboard::{Key, NamedKey},
};

impl TrayButton {
    pub(super) fn keyboard_input(&mut self, ctx: &mut EventCtx<'_>, event: &TextEvent) {
        if matches!(event, TextEvent::WindowFocusChange(false)) {
            self.cancel_press();
        }
        let TextEvent::Keyboard(event) = event else {
            return;
        };
        if self.appearance.activate_on_key_release
            && (event.key == Key::Named(NamedKey::Enter)
                || matches!(&event.key, Key::Character(key) if key == " "))
        {
            if event.state.is_down() {
                // A repeat received after a save moved focus cannot arm Close
                // or a re-enabled checkbox. A fresh sequence is required.
                if !event.repeat && ctx.is_focus_target() {
                    self.keyboard_press = Some((event.key.clone(), event.code));
                }
            } else if self.keyboard_press.take().is_some_and(|(key, code)| {
                key == event.key && code == event.code && ctx.is_focus_target()
            }) {
                self.submit(ctx, ButtonAction::Activate);
            }
            ctx.set_handled();
            return;
        }
        if !event.state.is_down() {
            return;
        }
        let action = match &event.key {
            Key::Named(NamedKey::ContextMenu) => Some(ButtonAction::Context),
            Key::Named(NamedKey::F10) if event.modifiers.shift() => Some(ButtonAction::Context),
            Key::Named(NamedKey::Escape) => Some(ButtonAction::Dismiss),
            Key::Named(NamedKey::ArrowUp) if !self.appearance.icon => Some(ButtonAction::Step(-1)),
            Key::Named(NamedKey::ArrowDown) if !self.appearance.icon => Some(ButtonAction::Step(1)),
            Key::Named(NamedKey::ArrowLeft) if !self.appearance.icon => Some(ButtonAction::Back),
            Key::Named(NamedKey::ArrowRight) if self.appearance.submenu => {
                Some(ButtonAction::Activate)
            }
            Key::Named(NamedKey::Enter) => Some(ButtonAction::Activate),
            Key::Character(key) if key == " " => Some(ButtonAction::Activate),
            _ => None,
        };
        if let Some(action) = action {
            self.submit(ctx, action);
            ctx.set_handled();
        }
    }
}
