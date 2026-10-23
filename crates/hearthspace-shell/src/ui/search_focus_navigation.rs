//! Arrow navigation supplements native Tab and activation in a choice popup.
use super::FocusTarget;
use xilem::masonry::core::{
    EventCtx, TextEvent,
    keyboard::{Key, NamedKey},
};

pub(super) fn handle(
    ctx: &mut EventCtx<'_>,
    event: &TextEvent,
    targets: &[FocusTarget],
    back: bool,
) -> bool {
    let TextEvent::Keyboard(event) = event else {
        return false;
    };
    if !event.state.is_down()
        || event.modifiers.ctrl()
        || event.modifiers.alt()
        || event.modifiers.meta()
    {
        return false;
    }
    if back && event.key == Key::Named(NamedKey::ArrowLeft) {
        ctx.submit_untyped_action(Box::new(super::GoBack));
        ctx.set_handled();
        return true;
    }
    let ids: Vec<_> = targets
        .iter()
        .filter_map(|target| *target.lock().unwrap())
        .collect();
    let Some(current) = ids.iter().position(|id| Some(*id) == ctx.focus_target_id()) else {
        return false;
    };
    let index = match event.key {
        Key::Named(NamedKey::ArrowUp) => (current + ids.len() - 1) % ids.len(),
        Key::Named(NamedKey::ArrowDown) => (current + 1) % ids.len(),
        Key::Named(NamedKey::Home) => 0,
        Key::Named(NamedKey::End) => ids.len() - 1,
        _ => return false,
    };
    ctx.set_focus(ids[index]);
    ctx.set_handled();
    true
}
