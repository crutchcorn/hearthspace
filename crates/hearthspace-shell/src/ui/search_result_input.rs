//! A menu opens only after the input sequence owned by this app control ends.
use super::{ResultButton, SelectionStep};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use xilem::masonry::core::{
    EventCtx, PointerButton, PointerEvent, PointerType, TextEvent, UpdateCtx,
    keyboard::{Code, Key, NamedKey},
};
use xilem::masonry::widgets::ButtonPress;

#[derive(Default)]
pub(in crate::ui::search_result) struct Input {
    pointer: Option<Press>,
    keyboard: Option<(Key, Code, bool)>,
    pending: Option<Arc<AtomicBool>>,
}

struct Press {
    button: Option<PointerButton>,
    origin: xilem::dpi::PhysicalPosition<f64>,
    touch: bool,
    elapsed: u64,
    cancelled: bool,
}

impl Input {
    pub(in crate::ui::search_result) fn cancel(&mut self) {
        self.pointer = None;
        self.keyboard = None;
        if let Some(pending) = self.pending.take() {
            pending.store(false, Ordering::Release);
        }
    }
}

impl ResultButton {
    pub(super) fn open_context(
        &mut self,
        ctx: &mut EventCtx<'_>,
        intent: crate::ui::app_context::Intent,
    ) {
        let Some(target) = self.context.clone() else {
            return;
        };
        self.input.cancel();
        let alive = Arc::new(AtomicBool::new(true));
        self.input.pending = Some(alive.clone());
        let mut request = crate::ui::app_context::new_request(target, ctx.bounding_box(), alive);
        request.intent = intent;
        ctx.submit_untyped_action(Box::new(request));
    }

    pub(super) fn pointer_input(&mut self, ctx: &mut EventCtx<'_>, event: &PointerEvent) {
        match event {
            PointerEvent::Down(event)
                if matches!(event.button, None | Some(PointerButton::Primary))
                    || (event.button == Some(PointerButton::Secondary)
                        && self.context.is_some()) =>
            {
                self.input.cancel();
                let touch = event.pointer.pointer_type == PointerType::Touch;
                self.input.pointer = Some(Press {
                    button: event.button,
                    origin: event.state.position,
                    touch,
                    elapsed: 0,
                    cancelled: false,
                });
                ctx.capture_pointer();
                ctx.request_render();
                if let Some(target) = *self.target.lock().unwrap() {
                    ctx.set_focus(target);
                } else {
                    ctx.request_focus();
                }
                if touch && self.context.is_some() {
                    ctx.request_anim_frame();
                }
            }
            PointerEvent::Move(event) => {
                if let Some(press) = &mut self.input.pointer
                    && press.touch
                    && (event.current.position.x - press.origin.x)
                        .hypot(event.current.position.y - press.origin.y)
                        > 8.0
                {
                    press.cancelled = true;
                }
            }
            PointerEvent::Up(event) => {
                if let Some(press) = self.input.pointer.take()
                    && ctx.is_active()
                    && ctx.is_hovered()
                    && !press.cancelled
                    && press.button == event.button
                {
                    if event.button == Some(PointerButton::Secondary)
                        || (press.touch && press.elapsed >= 500_000_000 && self.context.is_some())
                    {
                        self.open_context(ctx, crate::ui::app_context::Intent::Ordinary);
                    } else {
                        ctx.submit_action::<ButtonPress>(ButtonPress {
                            button: event.button,
                        });
                    }
                }
                ctx.request_render();
            }
            PointerEvent::Cancel(_) => self.input.cancel(),
            _ => {}
        }
    }

    pub(super) fn touch_frame(&mut self, ctx: &mut UpdateCtx<'_>, interval: u64) {
        if let Some(press) = &mut self.input.pointer
            && press.touch
            && !press.cancelled
            && ctx.is_active()
        {
            press.elapsed = press.elapsed.saturating_add(interval);
            if press.elapsed < 500_000_000 {
                ctx.request_anim_frame();
            }
        }
    }

    pub(super) fn text_input(&mut self, ctx: &mut EventCtx<'_>, event: &TextEvent) {
        if matches!(event, TextEvent::WindowFocusChange(false)) {
            self.input.cancel();
        }
        let TextEvent::Keyboard(event) = event else {
            return;
        };
        if self.navigation
            && event.state.is_down()
            && !event.modifiers.ctrl()
            && !event.modifiers.alt()
            && !event.modifiers.meta()
            && !event.modifiers.shift()
            && let Key::Named(
                key @ (NamedKey::ArrowDown
                | NamedKey::ArrowUp
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight),
            ) = event.key
        {
            if let Some(target) = *self.target.lock().unwrap() {
                ctx.set_focus(target);
            }
            let delta = match key {
                NamedKey::ArrowDown => self.columns as i32,
                NamedKey::ArrowUp => -(self.columns as i32),
                NamedKey::ArrowLeft => -1,
                _ => 1,
            };
            ctx.submit_untyped_action(Box::new(SelectionStep(delta)));
            ctx.set_handled();
            return;
        }
        let context = self.context.is_some()
            && (event.key == Key::Named(NamedKey::ContextMenu)
                || (event.key == Key::Named(NamedKey::F10) && event.modifiers.shift()));
        let activate =
            event.key == Key::Named(NamedKey::Enter) || event.key == Key::Character(" ".into());
        if event.state.is_down() && (context || activate) {
            if !event.repeat {
                self.input.keyboard = Some((event.key.clone(), event.code, context));
            }
            ctx.set_handled();
        } else if event.state.is_up()
            && self
                .input
                .keyboard
                .as_ref()
                .is_some_and(|(key, code, _)| key == &event.key && *code == event.code)
        {
            let (_, _, context) = self.input.keyboard.take().unwrap();
            if context {
                self.open_context(ctx, crate::ui::app_context::Intent::Ordinary);
            } else {
                if let Some(target) = *self.target.lock().unwrap() {
                    ctx.set_focus(target);
                }
                ctx.submit_action::<ButtonPress>(ButtonPress { button: None });
            }
            ctx.set_handled();
        }
    }
}
