//! The focused preview can select and arrange displays without pointer dragging.
use super::*;
use xilem::masonry::core::keyboard::{Key, NamedKey};

impl MonitorLayoutWidget {
    pub(super) fn handle_key(&mut self, ctx: &mut EventCtx<'_>, event: &TextEvent) {
        let TextEvent::Keyboard(event) = event else {
            return;
        };
        if !event.state.is_down()
            || !ctx.is_focus_target()
            || self.drag.is_some()
            || self.monitors.is_empty()
        {
            return;
        }
        let index = self
            .monitors
            .iter()
            .position(|monitor| self.selected_monitor_id.as_ref() == Some(&monitor.id))
            .unwrap_or(0);
        if event.key == Key::Character(" ".into()) {
            let index = (index + 1) % self.monitors.len();
            let id = self.monitors[index].id.clone();
            self.selected_monitor_id = Some(id.clone());
            ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Select {
                id,
                monitors: self.monitors.clone(),
            });
        } else {
            let Some(other) = self
                .monitors
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .min_by_key(|(_, other)| {
                    let m = &self.monitors[index];
                    (i64::from(m.x) - i64::from(other.x)).abs()
                        + (i64::from(m.y) - i64::from(other.y)).abs()
                })
                .map(|(_, m)| m)
            else {
                return;
            };
            let monitor = &self.monitors[index];
            let side = match event.key {
                Key::Named(NamedKey::ArrowLeft) => 0,
                Key::Named(NamedKey::ArrowRight) => 1,
                Key::Named(NamedKey::ArrowUp) => 2,
                Key::Named(NamedKey::ArrowDown) => 3,
                _ => return,
            };
            let Some((x, y)) = bordered_positions_for(monitor, other, other.x, other.y)[side]
            else {
                return;
            };
            let moved_id = monitor.id.clone();
            set_monitor_position(&mut self.monitors, &moved_id, x, y);
            normalize_monitors_near_origin(&mut self.monitors);
            ctx.submit_action::<MonitorEditorAction>(MonitorEditorAction::Drag {
                moved_id,
                monitors: self.monitors.clone(),
                commit: true,
            });
        }
        ctx.set_handled();
        ctx.request_render();
        ctx.request_accessibility_update();
    }
}
