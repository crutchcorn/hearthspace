//! Window actions shared by keyboard navigation, the shell, and agent control.
use smithay::{
    utils::{Logical, Point, Rectangle, Size},
    wayland::shell::xdg::ToplevelSurface,
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;

use super::{App, ManagedSurface, ManagedWindowKind};
use crate::{config::CONTROL_BAR_HEIGHT, geometry::CanvasPoint};

mod lifecycle;
mod metadata;
mod popups;
#[cfg(test)]
mod tests;

impl App {
    pub(super) fn is_navigable_window(&self, index: usize) -> bool {
        let window = &self.windows[index];
        window.kind == ManagedWindowKind::Normal
            && window.mapped
            && match &window.surface {
                ManagedSurface::Xdg(_) => true,
                #[cfg(feature = "xwayland")]
                ManagedSurface::X11(surface) => !surface.is_override_redirect(),
            }
    }

    pub(super) fn navigation_available(&self) -> bool {
        !self.session_locked()
            && !self.keyboard.is_grabbed()
            && !self.pointer.is_grabbed()
            && self.drag.is_none()
            && self.resize.is_none()
    }

    fn check_navigation_available(&self) -> Result<(), String> {
        self.navigation_available().then_some(()).ok_or_else(|| {
            "window action unavailable during an input grab or interactive move/resize".into()
        })
    }

    fn navigation_window_index(&self, id: u64) -> Result<usize, String> {
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or_else(|| format!("unknown window {id}"))?;
        self.is_navigable_window(index)
            .then_some(index)
            .ok_or_else(|| format!("window {id} is not a mapped application window"))
    }

    pub(super) fn focus_window(&mut self, id: u64) -> Result<(), String> {
        self.check_navigation_available()?;
        let index = self.navigation_window_index(id)?;
        self.interrupt_pinch_zoom();
        self.advance_viewport_animation();
        self.reveal_window(index, false);
        let index = self.raise_window(index);
        if let Some(surface) = self.windows[index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(index, surface);
        }
        self.request_redraw();
        Ok(())
    }

    pub(super) fn cycle_window_focus(&mut self, reverse: bool) -> Result<(), String> {
        self.check_navigation_available()?;
        let mut ids: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, _)| self.is_navigable_window(*index))
            .map(|(_, window)| window.id)
            .collect();
        // A stable order keeps repeated cycling from bouncing between the two
        // most recently raised windows. Closing a window simply removes its ID.
        ids.sort_unstable();
        let id = cycle_target(&ids, self.focused_normal_window_id, reverse)
            .ok_or_else(|| "no mapped application windows".to_string())?;
        self.focus_window(id)
    }

    pub(super) fn close_window(&mut self, id: u64) -> Result<(), String> {
        self.check_navigation_available()?;
        let index = self.navigation_window_index(id)?;
        self.windows[index].surface.send_close();
        Ok(())
    }

    pub(super) fn move_window(&mut self, id: u64, x: i32, y: i32) -> Result<(), String> {
        self.check_navigation_available()?;
        if !(-1_000_000..=1_000_000).contains(&x) || !(-1_000_000..=1_000_000).contains(&y) {
            return Err("canvas coordinates must be between -1000000 and 1000000".into());
        }
        let index = self.navigation_window_index(id)?;
        if self.windows[index].presentation.pinned() {
            return Err("restore the window before moving it".into());
        }
        if self.window_uses_output_coordinates(index) {
            return Err("restore the parent window before moving its dialog".into());
        }
        self.interrupt_pinch_zoom();
        let window = &mut self.windows[index];
        #[cfg(feature = "xwayland")]
        if let ManagedSurface::X11(surface) = &window.surface {
            let size = surface.geometry().size;
            surface
                .configure(Some(Rectangle::new((x, y).into(), size)))
                .map_err(|error| format!("X11 move failed: {error}"))?;
        }
        window.position = CanvasPoint { x, y };
        self.request_redraw();
        Ok(())
    }

    pub(super) fn resize_window(&mut self, id: u64, width: u32, height: u32) -> Result<(), String> {
        self.check_navigation_available()?;
        if !(1..=16_384).contains(&width) || !(1..=16_384).contains(&height) {
            return Err("window dimensions must be between 1 and 16384 logical pixels".into());
        }
        let index = self.navigation_window_index(id)?;
        if self.windows[index].presentation.pinned() {
            return Err("restore the window before resizing it".into());
        }
        self.interrupt_pinch_zoom();
        let size = Size::from((width as i32, height as i32));
        let window = &self.windows[index];
        match &window.surface {
            ManagedSurface::Xdg(surface) => configure_size(surface, size),
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(surface) => {
                surface
                    .configure(Some(Rectangle::new(
                        (window.position.x, window.position.y).into(),
                        size,
                    )))
                    .map_err(|error| format!("X11 resize failed: {error}"))?;
            }
        }
        self.request_redraw();
        Ok(())
    }

    pub(super) fn focus_launcher(&mut self) -> Result<(), String> {
        self.check_navigation_available()?;
        let target_output = self
            .capture_outputs()
            .into_iter()
            .find(|output| {
                Rectangle::new(
                    (output.logical_x, output.logical_y).into(),
                    (output.logical_width, output.logical_height).into(),
                )
                .contains(self.pointer_location.to_i32_round())
            })
            .map(|output| output.name);
        let index = self
            .windows
            .iter()
            .position(|window| {
                window.kind == ManagedWindowKind::ShellBar
                    && window.mapped
                    && window.shell_output_name == target_output
            })
            .or_else(|| {
                self.windows
                    .iter()
                    .position(|window| window.kind == ManagedWindowKind::ShellBar && window.mapped)
            })
            .ok_or_else(|| "application launcher is not ready".to_string())?;
        if let Some(surface) = self.windows[index].surface.wl_surface() {
            self.clear_keyboard_focus();
            self.set_keyboard_focus_to_window(index, surface);
        }
        Ok(())
    }

    pub(super) fn reset_view(&mut self) -> Result<(), String> {
        self.check_navigation_available()?;
        self.interrupt_pinch_zoom();
        self.viewport_animation = None;
        self.viewport_scale = 1.0;
        if let Some(index) = self
            .focused_normal_window_id
            .and_then(|id| self.navigation_window_index(id).ok())
        {
            self.reveal_window(index, true);
        } else {
            self.viewport_offset = CanvasPoint { x: 0, y: 0 };
        }
        self.request_redraw();
        Ok(())
    }

    fn reveal_window(&mut self, index: usize, center: bool) {
        if self.window_uses_output_coordinates(index) || self.windows[index].presentation.restoring
        {
            return;
        }
        let canvas = self.window_canvas_rect(index);
        let outputs = self.output_logical_rects();
        let usable: Vec<_> = outputs.iter().copied().map(usable_output).collect();
        let Some(screen) = self.normal_window_screen_rect(index) else {
            return;
        };
        if !center && usable.iter().any(|output| output.contains_rect(screen)) {
            return;
        }
        let target = usable
            .iter()
            .zip(&outputs)
            .find(|(_, output)| output.contains(self.pointer_location.to_i32_round()))
            .map(|(usable, _)| *usable)
            .unwrap_or_else(|| usable_output(self.primary_output_logical_rect()));
        self.viewport_animation = None;
        self.viewport_offset = reveal_offset(canvas, target, self.viewport_scale);
    }
}

fn configure_size(surface: &ToplevelSurface, size: Size<i32, Logical>) {
    surface.with_pending_state(|state| {
        state.states.unset(xdg_toplevel::State::Resizing);
        // An explicit user/agent resize supersedes advisory initial bounds.
        // None emits configure_bounds(0, 0), meaning unknown/unconstrained.
        state.bounds = None;
        state.size = Some(size);
    });
    surface.send_configure();
}

pub(super) fn usable_output(mut output: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let bar = CONTROL_BAR_HEIGHT.min(output.size.h.saturating_sub(1));
    output.loc.y += bar;
    output.size.h -= bar;
    output
}

pub(super) fn reveal_offset(
    canvas: Rectangle<i32, Logical>,
    output: Rectangle<i32, Logical>,
    scale: f64,
) -> CanvasPoint {
    let aligned = Point::<f64, Logical>::from((
        f64::from(output.loc.x)
            + ((f64::from(output.size.w) - f64::from(canvas.size.w) * scale) / 2.0).max(0.0),
        f64::from(output.loc.y)
            + ((f64::from(output.size.h) - f64::from(canvas.size.h) * scale) / 2.0).max(0.0),
    ));
    CanvasPoint {
        x: (f64::from(canvas.loc.x) - aligned.x / scale).round() as i32,
        y: (f64::from(canvas.loc.y) - aligned.y / scale).round() as i32,
    }
}

fn cycle_target(ids: &[u64], current: Option<u64>, reverse: bool) -> Option<u64> {
    if ids.is_empty() {
        return None;
    }
    let index = current.and_then(|id| ids.iter().position(|candidate| *candidate == id));
    let index = match index {
        Some(index) if reverse => (index + ids.len() - 1) % ids.len(),
        Some(index) => (index + 1) % ids.len(),
        None if reverse => ids.len() - 1,
        None => 0,
    };
    Some(ids[index])
}
