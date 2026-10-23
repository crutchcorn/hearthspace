//! Focused windows tuck the shell away until a corner or keyboard reveals it.
use std::time::{Duration, Instant};

use smithay::utils::{Logical, Point, Rectangle};

use super::{App, HitTarget, windows::TitlebarButton};

const CORNER_DISTANCE: f64 = 72.;
const HOVER_GRACE: Duration = Duration::from_millis(450);
const TOUCH_GRACE: Duration = Duration::from_millis(2500);

#[derive(Default)]
pub(super) struct FocusedShellPeek {
    window: Option<u64>,
    visible: bool,
    hide_at: Option<Instant>,
}

impl App {
    pub(in crate::compositor) fn focused_shell_peek_timeout(&self) -> Option<Duration> {
        // Menus and keyboard focus keep the shell visible until a real input or
        // lifecycle event arrives. An expired deadline must not busy-loop.
        if self.focused_shell_keeps_open() {
            return None;
        }
        self.focused_shell_peek
            .hide_at
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    pub(in crate::compositor) fn reset_focused_shell_peek(&mut self) {
        self.focused_shell_peek = FocusedShellPeek::default();
        self.request_redraw();
    }

    pub(in crate::compositor) fn reveal_focused_shell_for_keyboard(&mut self) {
        self.maintain_focused_shell_peek();
        if self.focused_shell_peek.window.is_some() {
            self.focused_shell_peek.visible = true;
            self.focused_shell_peek.hide_at = Some(Instant::now() + HOVER_GRACE);
            self.request_redraw();
        }
    }

    fn focused_shell_window(&self) -> Option<usize> {
        if self.session_locked() || self.canvas_preview_active() {
            return None;
        }
        self.windows.iter().rposition(|window| {
            window.mapped && window.presentation.focused_view && !window.presentation.fullscreen
        })
    }

    pub(in crate::compositor) fn focused_shell_visible_on_output(
        &self,
        output: Rectangle<i32, Logical>,
    ) -> bool {
        let Some(index) = self.focused_shell_window() else {
            return false;
        };
        self.presentation_output_rect(index) == Some(output)
            && self.focused_shell_peek.window == Some(self.windows[index].id)
            && self.focused_shell_peek.visible
    }

    pub(in crate::compositor) fn maintain_focused_shell_peek(&mut self) {
        let id = self
            .focused_shell_window()
            .map(|index| self.windows[index].id);
        if self.focused_shell_peek.window != id {
            self.focused_shell_peek = FocusedShellPeek {
                window: id,
                ..Default::default()
            };
            self.request_redraw();
        }
        if self
            .focused_shell_peek
            .hide_at
            .is_some_and(|deadline| Instant::now() >= deadline)
            && !self.focused_shell_keeps_open()
        {
            self.focused_shell_peek.hide_at = None;
            self.focused_shell_peek.visible = false;
            self.request_redraw();
        }
    }

    fn focused_shell_keeps_open(&self) -> bool {
        let focused = self
            .keyboard
            .current_focus()
            .and_then(|focus| self.window_index_for_surface(focus.surface()));
        focused.is_some_and(|index| self.windows[index].kind.is_shell_chrome())
            || self.windows.iter().enumerate().any(|(index, window)| {
                window.mapped && self.is_shell_panel(index) && !self.is_layout_surface(index)
            })
    }

    pub(in crate::compositor) fn focused_shell_pointer(
        &mut self,
        point: Point<f64, Logical>,
        touch: bool,
    ) {
        self.maintain_focused_shell_peek();
        let Some(index) = self.focused_shell_window() else {
            return;
        };
        let Some(output) = self.presentation_output_rect(index) else {
            return;
        };
        // The window's own restore and close buttons share the top-right reveal
        // region. They must stay clickable without the tray appearing over them.
        let chrome_button = self.has_compositor_chrome(index)
            && [TitlebarButton::Maximize, TitlebarButton::Close]
                .into_iter()
                .any(|button| {
                    self.titlebar_button_screen_rect(index, button)
                        .contains(point)
                });
        let in_shell = self.focused_shell_peek.visible
            && self.hit_test(point).is_some_and(|hit| {
                let index = match hit {
                    HitTarget::Client { window_index, .. }
                    | HitTarget::TitleBar { window_index }
                    | HitTarget::TitlebarButton { window_index, .. }
                    | HitTarget::ResizeBorder { window_index, .. } => window_index,
                };
                self.windows[index].kind.is_shell_chrome()
            });
        let reveal = in_shell || (!chrome_button && near_corner(point, output));
        let before = self.focused_shell_peek.visible;
        if reveal {
            self.focused_shell_peek.visible = true;
            self.focused_shell_peek.hide_at = touch.then(|| Instant::now() + TOUCH_GRACE);
        } else if self.focused_shell_peek.visible && self.focused_shell_peek.hide_at.is_none() {
            self.focused_shell_peek.hide_at = Some(Instant::now() + HOVER_GRACE);
        }
        if before != self.focused_shell_peek.visible {
            self.request_redraw();
        }
    }
}

fn near_corner(point: Point<f64, Logical>, output: Rectangle<i32, Logical>) -> bool {
    let bounds = output.to_f64();
    bounds.contains(point)
        && (point.x <= bounds.loc.x + CORNER_DISTANCE
            || point.x >= bounds.loc.x + bounds.size.w - CORNER_DISTANCE)
        && (point.y <= bounds.loc.y + CORNER_DISTANCE
            || point.y >= bounds.loc.y + bounds.size.h - CORNER_DISTANCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_reveal_uses_the_focused_output_origin_and_both_edges() {
        let output = Rectangle::new((1280, -200).into(), (1280, 800).into());
        assert!(near_corner((1281., -199.).into(), output));
        assert!(near_corner((2559., 599.).into(), output));
        assert!(!near_corner((1200., -190.).into(), output));
        assert!(!near_corner((1900., -190.).into(), output));
        assert!(!near_corner((1290., 300.).into(), output));
    }
}
