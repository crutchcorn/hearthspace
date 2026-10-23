//! A tap keeps primary activation; a stationary hold opens the app's menu.
use super::*;
use xilem::masonry::core::PointerType;

pub(super) struct Hold {
    origin: xilem::dpi::PhysicalPosition<f64>,
    elapsed: u64,
}

impl TrayButton {
    pub(super) fn track_touch(&mut self, event: &PointerEvent, ctx: &mut EventCtx<'_>) {
        match event {
            PointerEvent::Down(event)
                if self.appearance.icon && event.pointer.pointer_type == PointerType::Touch =>
            {
                self.touch_hold = Some(Hold {
                    origin: event.state.position,
                    elapsed: 0,
                });
                ctx.request_anim_frame();
            }
            PointerEvent::Move(event) => {
                if let Some(hold) = &self.touch_hold
                    && (event.current.position.x - hold.origin.x)
                        .hypot(event.current.position.y - hold.origin.y)
                        > 8.0
                {
                    self.touch_hold = None;
                    self.cancelled_press = true;
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => self.touch_hold = None,
            _ => {}
        }
    }

    pub(super) fn advance_touch_hold(&mut self, ctx: &mut UpdateCtx<'_>, interval: u64) {
        if !ctx.is_active() || !ctx.is_hovered() || self.cancelled_press || !self.appearance.enabled
        {
            self.touch_hold = None;
        }
        let Some(hold) = &mut self.touch_hold else {
            return;
        };
        hold.elapsed = hold.elapsed.saturating_add(interval);
        if hold.elapsed < 500_000_000 {
            ctx.request_anim_frame();
            return;
        }
        self.touch_hold = None;
        self.cancelled_press = true;
        let target = self.target.clone();
        ctx.mutate_self_later(move |mut widget| {
            widget.ctx.submit_action::<ButtonEvent>(ButtonEvent {
                target,
                action: ButtonAction::Context,
            });
        });
    }
}
