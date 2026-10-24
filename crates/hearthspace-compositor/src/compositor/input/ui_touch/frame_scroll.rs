//! A scroll takes ownership only after a drag; taps still reach the native control.
use super::*;

impl App {
    pub(super) fn motion_frame_scroll_ui_touch(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Position,
    ) -> bool {
        let Some(touch) = self
            .ui_touch
            .as_ref()
            .filter(|touch| touch.owns(device, slot))
        else {
            return false;
        };
        if !location.x.is_finite()
            || !location.y.is_finite()
            || !touch.output.contains(location.to_i32_round())
        {
            self.cancel_ui_touch();
            return true;
        }
        let (target, last, takeover) = match &touch.target {
            Target::FrameScroll { target, last } => (target.clone(), *last, false),
            Target::Client(_)
                if !self.layout_editing()
                    && !self.modal_confirmation_active()
                    && scroll_threshold(touch.origin, location) =>
            {
                let Some(target) =
                    self.layout_scroll_group_at(hearthspace_common::frame_layout::Point {
                        x: touch.origin.x,
                        y: touch.origin.y,
                    })
                else {
                    return false;
                };
                (target, touch.origin, true)
            }
            _ => return false,
        };
        if takeover {
            self.touch.clone().cancel(self);
        }
        self.ui_touch.as_mut().unwrap().target = Target::FrameScroll {
            target: target.clone(),
            last: location,
        };
        self.scroll_layout_group_motion(&target, location.x - last.x, location.y - last.y);
        true
    }

    pub(super) fn maintain_frame_scroll_ui_touch(&mut self) -> bool {
        let Some(touch) = self.ui_touch.as_ref() else {
            return false;
        };
        let Target::FrameScroll { target, .. } = &touch.target else {
            return false;
        };
        let target = target.clone();
        let output = touch.output;
        if !self.navigation_available()
            || !self.output_logical_rects().contains(&output)
            || !self.scroll_layout_group(&target, 0.)
        {
            self.cancel_ui_touch();
        }
        true
    }
}

fn scroll_threshold(start: Position, current: Position) -> bool {
    (current.x - start.x).hypot(current.y - start.y) >= 8.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn taps_remain_native_and_cancelled_scroll_cannot_become_a_click() {
        let start = (150., 70.).into();
        assert!(!scroll_threshold(start, (155., 74.).into()));
        assert!(scroll_threshold(start, (158., 70.).into()));
        let mut touch = super::super::tests::session();
        touch.target = Target::FrameScroll {
            target: crate::compositor::layout_editor::Target {
                output: "test".into(),
                target: hearthspace_common::frame_layout::LayoutTarget::new(
                    hearthspace_common::frame_layout::TargetKind::Group,
                    "system",
                ),
            },
            last: start,
        };
        assert!(
            !touch.cancel(),
            "the client already received cancel at scroll takeover"
        );
        assert!(matches!(touch.target, Target::Cancelled));
        assert!(touch.suppresses_pointer("screen", &TouchAction::Release));
        assert!(touch.suppresses_pointer("screen", &TouchAction::Tap(start)));
    }
}
