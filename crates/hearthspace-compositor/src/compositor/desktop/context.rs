//! Context menus use the same output-space palette policy as the shell popovers.
use super::App;
use crate::compositor::{
    CalloopData, HitTarget, shell_integration::ControlReply, windows::window_app_id,
};
use hearthspace_common::desktop_context::{self, DesktopContext};
use smithay::{
    backend::input::ButtonState,
    utils::{Logical, Point},
};

#[derive(Default)]
pub(super) struct State {
    pub(super) current: Option<DesktopContext>,
    sequence: u64,
    anchor: Point<i32, Logical>,
    suppress_release: bool,
    return_focus: Option<String>,
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
            || self.overview_active()
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
        self.desktop
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

    pub(in crate::compositor) fn cancel_pending_desktop_context(&mut self) {
        if let Some(context) = &self.desktop.context.current {
            self.close_desktop_context(context.generation);
        }
    }

    pub(in crate::compositor) fn reset_desktop_context(&mut self) {
        self.desktop.context.current = None;
        self.desktop.context.return_focus = None;
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
        let anchor = self.desktop.context.anchor;
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
            let menu_focused = self
                .keyboard
                .current_focus()
                .and_then(|focus| self.window_index_for_surface(focus.surface()))
                .and_then(|index| window_app_id(&self.windows[index].surface))
                .as_deref()
                .and_then(desktop_context::identity)
                .is_some_and(|(id, _)| id == generation);
            if menu_focused
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
}

impl CalloopData {
    pub(in crate::compositor) fn run_desktop_context_command(
        &mut self,
        text: &str,
    ) -> Option<ControlReply> {
        if !text.starts_with("desktop-context-") {
            return None;
        }
        if !self.running || !self.capture_active() || self.state.session_locked() {
            return Some(ControlReply::Err("Desktop is inactive".into()));
        }
        let Some((command, generation)) = text.split_once(' ') else {
            return Some(ControlReply::Err("Invalid desktop menu command".into()));
        };
        let Some(generation) = generation.parse::<u64>().ok() else {
            return Some(ControlReply::Err("Invalid desktop menu generation".into()));
        };
        let app = &mut self.state;
        if command == "desktop-context-close" {
            app.close_desktop_context(generation);
            return Some(ControlReply::Ok);
        }
        let Some(context) = app
            .desktop
            .context
            .current
            .as_ref()
            .filter(|c| c.generation == generation)
            .cloned()
        else {
            return Some(ControlReply::Err(
                "Desktop menu is no longer available".into(),
            ));
        };
        if context.ids.iter().any(|id| app.desktop_index(id).is_none()) {
            return Some(ControlReply::Err(
                "Desktop menu item is no longer available".into(),
            ));
        }
        if matches!(command, "desktop-context-copy" | "desktop-context-cut") {
            return Some(
                app.copy_desktop_files(&context.ids, command == "desktop-context-cut")
                    .map_or_else(ControlReply::Err, |_| ControlReply::Ok),
            );
        }
        if command == "desktop-context-select-all" {
            app.desktop.selected = app
                .desktop
                .order
                .iter()
                .filter(|id| app.desktop_index(id).is_some())
                .cloned()
                .collect();
            app.desktop_changed();
            return Some(ControlReply::Ok);
        }
        if command == "desktop-context-arrange" {
            for (slot, id) in app.desktop.order.clone().iter().enumerate() {
                let point = crate::geometry::CanvasPoint {
                    x: 28 + 104 * (slot / 4) as i32,
                    y: 112 + 104 * (slot % 4) as i32,
                };
                if let Some(index) = app.desktop_index(id) {
                    app.windows[index].position = point;
                }
                app.layout_store.remember_desktop(id, point);
            }
            app.desktop_changed();
            return Some(ControlReply::Ok);
        }
        if command == "desktop-context-background" {
            return Some(
                app.open_background_settings(context.output.as_deref())
                    .map_or_else(ControlReply::Err, |_| ControlReply::Ok),
            );
        }
        if command != "desktop-context-read" {
            return Some(ControlReply::Err("Invalid desktop menu command".into()));
        }
        Some(ControlReply::Payload(
            serde_json::to_vec(&serde_json::json!({
                "context":context,"xwayland_display":app.xwayland_display_name(),
            }))
            .unwrap(),
        ))
    }
}
