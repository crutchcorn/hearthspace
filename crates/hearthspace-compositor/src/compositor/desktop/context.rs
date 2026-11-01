//! Context menus use the same output-space palette policy as the shell popovers.
use super::App;
use crate::compositor::{HitTarget, windows::window_app_id};
use hearthspace_common::desktop_context::{self, DesktopContext};
use smithay::{
    backend::input::ButtonState,
    utils::{Logical, Point},
};

mod app;
mod commands;

#[derive(Default)]
pub(super) struct State {
    pub(super) current: Option<DesktopContext>,
    app_ids: std::collections::BTreeSet<String>,
    sequence: u64,
    anchor: Point<i32, Logical>,
    suppress_release: bool,
    return_focus: Option<String>,
    source_window: Option<u64>,
    root_window: Option<u64>,
}

impl App {
    pub(in crate::compositor) fn desktop_context_button(
        &mut self,
        button: u32,
        state: ButtonState,
    ) -> bool {
        if state == ButtonState::Pressed
            && let Some(context) = self.desktop.context.current.as_ref()
        {
            let generation = context.generation;
            let inside = matches!(self.hit_test(self.pointer_location), Some(HitTarget::Client { window_index, .. })
                if window_app_id(&self.windows[window_index].surface).as_deref()
                    .and_then(desktop_context::identity).is_some_and(|(id, _)| id == generation));
            if !inside {
                self.close_desktop_context(generation);
            }
        }
        if button != 0x111 {
            return false;
        }
        if state == ButtonState::Released {
            return std::mem::take(&mut self.desktop.context.suppress_release);
        }
        if !self.navigation_available()
            || self.pointer.is_grabbed()
            || self.layout_editing()
            || self.canvas_search_active()
            || self.window_switcher_active()
            || self.desktop.gesture.is_some()
            || self.group_interaction_active()
            || self.output_logical_rects().into_iter().any(|output| {
                output.contains(self.pointer_location.to_i32_round())
                    && self.fullscreen_on_output(output)
            })
        {
            return false;
        }
        let id = match self.hit_test(self.pointer_location) {
            Some(HitTarget::Client { window_index, .. }) => {
                let Some(id) = self.desktop_id(window_index) else {
                    return false;
                };
                Some(id)
            }
            None => None,
            _ => return false,
        };
        if self.open_desktop_context(id, false).is_err() {
            return false;
        }
        self.desktop.context.suppress_release = true;
        true
    }

    pub(in crate::compositor) fn open_desktop_context(
        &mut self,
        id: Option<String>,
        keyboard: bool,
    ) -> Result<(), String> {
        self.check_navigation_available()?;
        let anchor = if let Some(id) = &id {
            let index = self
                .desktop_index(id)
                .ok_or("Desktop item is unavailable")?;
            if !self.desktop.selected.contains(id) {
                self.desktop.selected.clear();
                self.desktop.selected.insert(id.clone());
            }
            self.focus_desktop(id, keyboard)?;
            let bounds = self.window_screen_bounds(index);
            if keyboard {
                bounds.loc + Point::from((bounds.size.w / 2, bounds.size.h / 2))
            } else {
                self.pointer_location.to_i32_round()
            }
        } else {
            self.desktop.selected.clear();
            self.pointer_location.to_i32_round()
        };
        let outputs = self.capture_outputs();
        let output = outputs
            .iter()
            .find(|o| {
                anchor.x >= o.logical_x
                    && anchor.y >= o.logical_y
                    && anchor.x < o.logical_x + o.logical_width
                    && anchor.y < o.logical_y + o.logical_height
            })
            .or_else(|| outputs.first())
            .ok_or("No active output")?;
        self.desktop.context.sequence = self.desktop.context.sequence.wrapping_add(1).max(1);
        self.desktop.context.anchor = anchor;
        self.desktop.context.return_focus = id;
        self.desktop.context.source_window = None;
        self.desktop.context.root_window = None;
        self.desktop.context.current = Some(DesktopContext {
            generation: self.desktop.context.sequence,
            ids: self.desktop.selected.iter().cloned().collect(),
            output: Some(output.name.clone()),
            max_width: output.logical_width.saturating_sub(16).max(1),
            max_height: output.logical_height.saturating_sub(64).max(1),
        });
        self.desktop.last_click = None;
        self.desktop_changed();
        Ok(())
    }

    /// Reject a menu created after dismissal, replacement, or an output change.
    pub(in crate::compositor) fn desktop_context_mapping_allowed(&self, index: usize) -> bool {
        let Some((generation, output)) = window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(desktop_context::identity)
        else {
            return true;
        };
        self.desktop_context_source_available()
            && self
                .desktop
                .context
                .current
                .as_ref()
                .is_some_and(|context| context.generation == generation && context.output == output)
            && self
                .output_logical_rect_for_name(output.as_deref())
                .is_some()
    }

    pub(in crate::compositor) fn desktop_context_pending(&self) -> bool {
        self.desktop.context.current.is_some()
            && !self.windows.iter().enumerate().any(|(index, window)| {
                window.mapped
                    && window_app_id(&window.surface)
                        .as_deref()
                        .and_then(desktop_context::identity)
                        .is_some()
                    && self.desktop_context_mapping_allowed(index)
            })
    }

    pub(in crate::compositor) fn desktop_context_mapped(&mut self, index: usize) {
        if window_app_id(&self.windows[index].surface).is_some_and(|id| {
            desktop_context::submenu_offset(&id).is_none()
                && desktop_context::identity(&id).is_some_and(|(generation, _)| {
                    self.desktop
                        .context
                        .current
                        .as_ref()
                        .is_some_and(|c| c.generation == generation)
                })
        }) {
            self.desktop.context.root_window = Some(self.windows[index].id);
        }
    }

    pub(in crate::compositor) fn cancel_pending_desktop_context(&mut self) {
        if let Some(context) = &self.desktop.context.current {
            self.close_desktop_context(context.generation);
        }
    }

    pub(in crate::compositor) fn reset_desktop_context(&mut self) {
        self.desktop.context.current = None;
        self.desktop.context.return_focus = None;
        self.desktop.context.source_window = None;
        self.desktop.context.root_window = None;
        self.desktop.context.suppress_release = false;
    }

    pub(in crate::compositor) fn desktop_context_origin(
        &self,
        index: usize,
    ) -> Option<Point<i32, Logical>> {
        let id = window_app_id(&self.windows[index].surface)?;
        let (generation, output) = desktop_context::identity(&id)?;
        let context = self.desktop.context.current.as_ref()?;
        if context.generation != generation || context.output != output {
            return None;
        }
        let output = self.output_logical_rect_for_name(output.as_deref())?;
        let size = self.window_geometry_size(index);
        let min = output.loc + Point::from((8, 8));
        let max = output.loc
            + Point::from((
                (output.size.w - size.w - 8).max(8),
                (output.size.h - size.h - 8).max(8),
            ));
        let anchor = if let Some(offset) = desktop_context::submenu_offset(&id) {
            let parent_id = desktop_context::app_id(generation, context.output.as_deref());
            let parent = self.windows.iter().position(|window| {
                window.mapped && window_app_id(&window.surface).as_deref() == Some(&parent_id)
            })?;
            let parent_loc =
                self.desktop_context_origin(parent)? + self.window_surface_geometry_loc(parent);
            let right = parent_loc.x + self.window_geometry_size(parent).w;
            let left = parent_loc.x - size.w;
            Point::from((
                if right <= max.x { right } else { left },
                parent_loc.y + offset,
            ))
        } else {
            self.desktop.context.anchor
        };
        Some(
            Point::from((anchor.x.clamp(min.x, max.x), anchor.y.clamp(min.y, max.y)))
                - self.window_surface_geometry_loc(index),
        )
    }

    pub(in crate::compositor) fn close_desktop_context(&mut self, generation: u64) {
        if self
            .desktop
            .context
            .current
            .as_ref()
            .is_some_and(|c| c.generation == generation)
        {
            self.desktop.context.current = None;
            let return_focus = self.desktop.context.return_focus.take();
            let source_window = self.desktop.context.source_window.take();
            let root_window = self.desktop.context.root_window.take();
            let menu_focused = self
                .keyboard
                .current_focus()
                .and_then(|focus| self.window_index_for_surface(focus.surface()))
                .is_some_and(|index| {
                    root_window == Some(self.windows[index].id)
                        || window_app_id(&self.windows[index].surface)
                            .as_deref()
                            .and_then(desktop_context::identity)
                            .is_some_and(|(id, _)| id == generation)
                });
            let restored_source =
                menu_focused && source_window.is_some_and(|id| self.restore_app_context_source(id));
            if menu_focused
                && !restored_source
                && return_focus
                    .as_deref()
                    .is_none_or(|id| self.focus_desktop(id, true).is_err())
            {
                self.clear_keyboard_focus();
            }
            for window in &self.windows {
                if window_app_id(&window.surface)
                    .as_deref()
                    .and_then(desktop_context::identity)
                    .is_some_and(|(id, _)| id == generation)
                {
                    window.surface.send_close();
                }
            }
            self.desktop_changed();
        }
    }
    /// Teardown can precede close IPC. Restore the live parent while the retiring
    /// surface still owns focus, without transferring a later outside focus.
    pub(in crate::compositor) fn restore_desktop_context_parent(
        &mut self,
        surface: &wayland_server::protocol::wl_surface::WlSurface,
    ) -> bool {
        if self
            .keyboard
            .current_focus()
            .is_none_or(|focus| focus.surface() != surface)
        {
            return false;
        }
        let Some(index) = self.window_index_for_surface(surface) else {
            return false;
        };
        if let Some(id) = window_app_id(&self.windows[index].surface)
            && desktop_context::submenu_offset(&id).is_some()
            && let Some((generation, _)) = desktop_context::identity(&id)
        {
            return self.focus_desktop_context_menu(generation, false);
        }
        // Smithay clears role attributes on null-buffer unmap. The accepted
        // mapping's live window ID still identifies this generation's root.
        if self.desktop.context.root_window != Some(self.windows[index].id) {
            return false;
        }
        let Some(source) = self.desktop.context.source_window.or_else(|| {
            self.desktop
                .context
                .return_focus
                .as_deref()
                .and_then(|id| self.desktop_index(id))
                .map(|index| self.windows[index].id)
        }) else {
            return false;
        };
        let Some(generation) = self.desktop.context.current.as_ref().map(|c| c.generation) else {
            return false;
        };
        self.close_desktop_context(generation);
        self.keyboard.current_focus().is_some_and(|focus| {
            self.window_index_for_surface(focus.surface())
                .is_some_and(|index| self.windows[index].id == source)
        })
    }

    fn focus_desktop_context_menu(&mut self, generation: u64, child: bool) -> bool {
        let Some(context) = self
            .desktop
            .context
            .current
            .as_ref()
            .filter(|context| context.generation == generation)
        else {
            return false;
        };
        let parent_id = desktop_context::app_id(generation, context.output.as_deref());
        if let Some(index) = self.windows.iter().position(|window| {
            window.mapped
                && window_app_id(&window.surface).is_some_and(|id| {
                    if child {
                        desktop_context::identity(&id) == desktop_context::identity(&parent_id)
                            && desktop_context::submenu_offset(&id).is_some()
                    } else {
                        id == parent_id
                    }
                })
        }) && let Some(surface) = self.windows[index].surface.wl_surface()
        {
            self.set_keyboard_focus_to_window(index, surface);
            return true;
        }
        false
    }
}
