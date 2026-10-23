//! Canvas maximization and output-focused states share one restore transaction.
use smithay::{
    utils::{Logical, Rectangle, Size},
    wayland::shell::xdg::ToplevelSurface,
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;

use super::{App, ManagedSurface, ManagedWindowKind};

mod configure;
mod focused;
mod geometry;
mod groups;
#[cfg(test)]
mod tests;
mod transform;
mod transients;

#[derive(Debug, Default)]
pub(super) struct WindowPresentation {
    pub(super) maximized: bool,
    pub(super) fullscreen: bool,
    pub(super) focused_view: bool,
    pub(super) maximized_bounds: Option<Rectangle<i32, Logical>>,
    pub(super) output: Option<String>,
    pub(super) restoring: bool,
    focused_output_bounds: Option<Rectangle<i32, Logical>>,
    reveal_on_restore: bool,
    restore_size: Option<Size<i32, Logical>>,
}

impl WindowPresentation {
    pub(super) fn pinned(&self) -> bool {
        self.maximized || self.fullscreen
    }

    pub(super) fn output_pinned(&self) -> bool {
        self.fullscreen || self.focused_view
    }

    fn changes_for_request(
        &self,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
        output: &str,
    ) -> bool {
        let fullscreen = fullscreen.unwrap_or(self.fullscreen);
        let maximized = maximized.unwrap_or(self.maximized);
        fullscreen != self.fullscreen
            || maximized != self.maximized
            || ((fullscreen || maximized) && self.output.as_deref() != Some(output))
    }

    fn update(
        &mut self,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
        size: Size<i32, Logical>,
    ) {
        let was_pinned = self.pinned();
        if let Some(fullscreen) = fullscreen {
            self.fullscreen = fullscreen;
        }
        if let Some(maximized) = maximized {
            self.maximized = maximized;
            if !maximized {
                self.focused_view = false;
                self.focused_output_bounds = None;
                self.maximized_bounds = None;
            }
        }
        if self.pinned() {
            if !was_pinned && !self.restoring {
                self.restore_size = (size.w > 0 && size.h > 0).then_some(size);
            }
            self.restoring = false;
            self.reveal_on_restore = false;
        } else if was_pinned {
            self.restoring = true;
            self.reveal_on_restore = true;
        }
    }
}

impl App {
    pub(super) fn user_window_presentation(
        &mut self,
        id: u64,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
    ) -> Result<(), String> {
        if !self.navigation_available() {
            return Err("window state unavailable during an input grab".into());
        }
        let Some(index) = self.windows.iter().position(|window| window.id == id) else {
            return Err(format!("unknown window {id}"));
        };
        if !self.is_navigable_window(index) {
            return Err("window is not a mapped application window".into());
        }
        self.update_window_presentation(id, fullscreen, maximized, None)?;
        self.focus_presentation_window(id)
    }
    pub(super) fn set_window_fullscreen(
        &mut self,
        id: u64,
        enabled: bool,
        output: Option<String>,
    ) -> Result<(), String> {
        self.update_window_presentation(id, Some(enabled), None, output)
    }

    pub(super) fn set_window_maximized(&mut self, id: u64, enabled: bool) -> Result<(), String> {
        self.update_window_presentation(id, None, Some(enabled), None)
    }

    fn update_window_presentation(
        &mut self,
        id: u64,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
        output: Option<String>,
    ) -> Result<(), String> {
        self.update_window_presentation_with_focus(id, fullscreen, maximized, None, output)
    }

    fn update_window_presentation_with_focus(
        &mut self,
        id: u64,
        fullscreen: Option<bool>,
        maximized: Option<bool>,
        focused: Option<bool>,
        output: Option<String>,
    ) -> Result<(), String> {
        if self.session_locked() {
            return Err("session is locked".into());
        }
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id)
            .ok_or_else(|| format!("unknown window {id}"))?;
        if self.windows[index].kind != ManagedWindowKind::Normal {
            return Err("shell surfaces cannot change window state".into());
        }
        #[cfg(feature = "xwayland")]
        if let ManagedSurface::X11(surface) = &self.windows[index].surface
            && surface.is_override_redirect()
        {
            return Err("override-redirect windows cannot change window state".into());
        }
        if self.drag.as_ref().is_some_and(|drag| drag.window_id == id)
            || self
                .resize
                .as_ref()
                .is_some_and(|resize| resize.window_id == id)
        {
            return Err("cannot change state during an interactive move or resize".into());
        }
        let target = self.presentation_output_for_window(index, output.as_deref());
        if focused.is_none_or(|value| value == self.windows[index].presentation.focused_view)
            && !self.windows[index]
                .presentation
                .changes_for_request(fullscreen, maximized, &target)
        {
            // GTK sends unset_fullscreen before requesting a titlebar move,
            // including for floating windows. Acknowledge the request without
            // releasing its touch-emulated pointer grab or resetting pending
            // geometry/state belonging to another configure.
            if let ManagedSurface::Xdg(surface) = &self.windows[index].surface
                && surface.is_initial_configure_sent()
            {
                surface.send_configure();
            }
            return Ok(());
        }
        self.cancel_canvas_peek_motion(id);
        // A different window can change presentation while this finger owns
        // a client move/resize (or has not requested it yet). Stop canvas zoom
        // without releasing that window's pointer sequence.
        self.prepare_window_pointer_operation();
        if focused == Some(true) {
            self.advance_viewport_animation();
            self.viewport_animation = None;
        }
        let size = self.window_geometry_size(index);
        let maximize = maximized.unwrap_or(self.windows[index].presentation.maximized);
        let new_bounds = (maximize
            && (self.windows[index].presentation.maximized_bounds.is_none()
                || focused == Some(true)))
        .then(|| self.maximized_canvas_bounds(index, &target));
        self.windows[index].pending_resize = None;
        let state = &mut self.windows[index].presentation;
        state.update(fullscreen, maximized, size);
        if let Some(focused) = focused {
            state.focused_view = focused && state.maximized;
        }
        if let Some(bounds) = new_bounds {
            state.maximized_bounds = Some(bounds);
        }
        if state.pinned() {
            state.output = Some(target);
        }
        if focused == Some(true) {
            self.clear_window_group();
            self.clear_desktop_selection();
            self.reset_focused_shell_peek();
        }
        self.refresh_focused_output_bounds(index);
        self.configure_window_presentation(index);
        if !self.windows[index].presentation.pinned() {
            self.windows[index].presentation.output = None;
        }
        self.request_redraw();
        Ok(())
    }

    fn presentation_output_for_window(&self, index: usize, requested: Option<&str>) -> String {
        let outputs = self.capture_outputs();
        let state = &self.windows[index].presentation;
        if let Some(name) = requested.or_else(|| {
            state
                .output_pinned()
                .then_some(state.output.as_deref())
                .flatten()
        }) && outputs.iter().any(|output| output.name == name)
        {
            return name.to_string();
        }
        let rect = self.normal_window_screen_rect(index);
        outputs
            .iter()
            .filter_map(|output| {
                let bounds = Rectangle::new(
                    (output.logical_x, output.logical_y).into(),
                    (output.logical_width, output.logical_height).into(),
                );
                rect.and_then(|rect| rect.intersection(bounds))
                    .map(|rect| (output, i64::from(rect.size.w) * i64::from(rect.size.h)))
            })
            .max_by_key(|(_, overlap)| *overlap)
            .map(|(output, _)| output)
            .or_else(|| outputs.first())
            .map(|output| output.name.clone())
            .unwrap_or_default()
    }

    pub(super) fn presentation_output_rect(&self, index: usize) -> Option<Rectangle<i32, Logical>> {
        let state = &self.windows[index].presentation;
        state.output_pinned().then(|| {
            self.output_logical_rect_for_name(state.output.as_deref())
                .unwrap_or_else(|| self.primary_output_logical_rect())
        })
    }

    pub(super) fn reconcile_window_presentations(&mut self) {
        let outputs = self.capture_outputs();
        let Some(primary) = outputs.first() else {
            return;
        };
        for index in 0..self.windows.len() {
            let state = &mut self.windows[index].presentation;
            if !state.pinned() {
                continue;
            }
            if !outputs
                .iter()
                .any(|output| Some(&output.name) == state.output.as_ref())
            {
                state.output = Some(primary.name.clone());
            }
            self.refresh_focused_output_bounds(index);
            self.configure_window_presentation(index);
        }
    }

    pub(super) fn finish_presentation_restore(
        &mut self,
        root: &wayland_server::protocol::wl_surface::WlSurface,
    ) {
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.surface.wl_surface().as_ref() == Some(root))
        else {
            return;
        };
        if !self.windows[index].presentation.restoring {
            return;
        }
        let size = self.window_geometry_size(index);
        let ready = match &self.windows[index].surface {
            ManagedSurface::Xdg(surface) => surface.with_committed_state(|state| {
                state.is_some_and(|state| {
                    !state.states.contains(xdg_toplevel::State::Fullscreen)
                        && !state.states.contains(xdg_toplevel::State::Maximized)
                })
            }),
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(_) => self.windows[index].presentation.restore_size == Some(size),
        };
        if ready && size.w > 0 && size.h > 0 {
            let reveal = self.windows[index].presentation.reveal_on_restore;
            self.windows[index].presentation.restoring = false;
            self.windows[index].presentation.reveal_on_restore = false;
            self.windows[index].presentation.restore_size = None;
            let id = self.windows[index].id;
            if reveal && self.focused_normal_window_id == Some(id) {
                let _ = self.focus_window(id);
            }
        }
    }

    pub(super) fn xdg_window_id(&self, surface: &ToplevelSurface) -> Option<u64> {
        self.windows
            .iter()
            .find(|window| window.surface.same_xdg(surface))
            .map(|window| window.id)
    }
}

fn set_state(
    states: &mut smithay::wayland::shell::xdg::ToplevelStateSet,
    flag: xdg_toplevel::State,
    enabled: bool,
) {
    if enabled {
        states.set(flag);
    } else {
        states.unset(flag);
    }
}
