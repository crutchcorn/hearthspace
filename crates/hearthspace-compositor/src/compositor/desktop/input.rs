use super::{App, Gesture, ManagedWindowKind, geometry};
use crate::{
    compositor::{HitTarget, cursor::CursorIcon},
    geometry::CanvasPoint,
};
use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::ButtonEvent,
    utils::{Logical, Point, SERIAL_COUNTER},
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

impl App {
    fn desktop_pointer_id(&self) -> Option<String> {
        match self.hit_test(self.pointer_location) {
            Some(HitTarget::Client { window_index, .. }) => self.desktop_id(window_index),
            _ => None,
        }
    }

    fn desktop_canvas_pointer(&self) -> Point<f64, Logical> {
        (
            self.raw_pointer_location.x / self.viewport_scale + f64::from(self.viewport_offset.x),
            self.raw_pointer_location.y / self.viewport_scale + f64::from(self.viewport_offset.y),
        )
            .into()
    }

    pub(in crate::compositor) fn desktop_pointer_button(
        &mut self,
        button: u32,
        state: ButtonState,
        time: InputTime,
    ) -> bool {
        if button != 0x110 {
            return false;
        }
        if state == ButtonState::Released {
            if self.desktop.suppress_release {
                self.desktop.suppress_release = false;
                self.release_desktop_pointer(button, time);
                return true;
            }
            let Some(gesture) = self.desktop.gesture.take() else {
                return false;
            };
            let app_drop = self.finish_desktop_drop();
            self.release_desktop_pointer(button, time);
            let mut positions_changed = false;
            match gesture {
                Gesture::Move {
                    id,
                    positions,
                    moved,
                    clickable,
                    ..
                } => {
                    if moved {
                        positions_changed = true;
                        if app_drop {
                            self.restore_desktop_positions(&positions);
                        } else if id.starts_with("app:") && self.desktop_over_apps_corner() {
                            self.restore_desktop_positions(&positions);
                            let _ = self.desktop_action("favorite", &id);
                        } else {
                            for (id, point) in self.desktop_positions() {
                                self.layout_store.remember_desktop(&id, point);
                                self.desktop.fresh.remove(&id);
                            }
                        }
                        self.desktop.last_click = None;
                    } else if clickable && self.desktop_pointer_id().as_deref() == Some(&id) {
                        let now = Instant::now();
                        let double =
                            self.desktop
                                .last_click
                                .as_ref()
                                .is_some_and(|(previous, at)| {
                                    previous == &id
                                        && now.duration_since(*at) <= Duration::from_millis(500)
                                });
                        self.desktop.last_click = (!double).then_some((id.clone(), now));
                        if double {
                            let _ = self.desktop_action("activate", &id);
                        }
                    }
                }
                Gesture::Marquee { .. } => self.desktop.last_click = None,
            }
            if positions_changed {
                self.desktop_changed();
            } else {
                self.request_redraw();
            }
            self.apply_pointer_motion(self.pointer_location, time);
            return true;
        }
        if self.session_locked()
            || self.overview_active()
            || self.window_switcher_active()
            || self.group_interaction_active()
            || self.pointer.is_grabbed()
        {
            return false;
        }
        let target = self.hit_test(self.pointer_location);
        let id = match &target {
            Some(HitTarget::Client { window_index, .. }) => self.desktop_id(*window_index),
            _ => None,
        };
        if target.is_some() && id.is_none() {
            return false;
        }
        if self.output_logical_rects().into_iter().any(|output| {
            output.contains(self.pointer_location.to_i32_round())
                && self.fullscreen_on_output(output)
        }) {
            return false;
        }
        let keyboard_focus_changed = std::mem::replace(&mut self.desktop.keyboard_focus, false);
        let modifiers = self.keyboard.modifier_state();
        let additive = modifiers.shift || modifiers.ctrl || modifiers.logo;
        if let Some(id) = id {
            if additive {
                if !self.desktop.selected.remove(&id) {
                    self.desktop.selected.insert(id.clone());
                }
                self.desktop.last_click = None;
            } else if !self.desktop.selected.contains(&id) {
                self.desktop.selected.clear();
                self.desktop.selected.insert(id.clone());
            }
            let selected: Vec<_> = self.desktop.selected.iter().cloned().collect();
            for id in selected {
                if let Some(index) = self.desktop_index(&id) {
                    self.grab_canvas_peek_target(index);
                }
            }
            self.desktop.gesture = Some(Gesture::Move {
                id,
                start: self.desktop_canvas_pointer(),
                positions: self.desktop_positions(),
                moved: false,
                clickable: !additive,
            });
            self.desktop.edge_tick = Instant::now();
            self.desktop_changed();
            // Native delivery requests the tile's actual Masonry focus. The tile never activates on release.
            false
        } else {
            let original = self.desktop.selected.clone();
            let before = if additive {
                self.desktop.selected.clone()
            } else {
                BTreeSet::new()
            };
            let selection_changed = before != original;
            self.desktop.selected = before.clone();
            self.desktop.last_click = None;
            self.clear_keyboard_focus();
            if self.desktop.select_mode {
                let pointer = self.desktop_canvas_pointer();
                self.desktop.gesture = Some(Gesture::Marquee {
                    start: pointer,
                    current: pointer,
                    before,
                    original,
                });
                self.desktop.edge_tick = Instant::now();
            }
            if selection_changed || keyboard_focus_changed {
                self.desktop_changed();
            } else {
                self.request_redraw();
            }
            self.desktop.select_mode
        }
    }

    fn release_desktop_pointer(&mut self, button: u32, time: InputTime) {
        if self.pressed_pointer_buttons.contains(&button) {
            self.deliver_pointer_button(&ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time,
                button,
                state: ButtonState::Released,
            });
            self.pointer.clone().frame(self);
        }
    }

    pub(in crate::compositor) fn desktop_over_apps_corner(&self) -> bool {
        self.windows.iter().enumerate().any(|(index, window)| {
            window.mapped
                && window.kind == ManagedWindowKind::ShellBar
                && (self.shell_corner(index) == Some(hearthspace_common::ShellCorner::Apps)
                    || self
                        .frame_widget_identity(index)
                        .is_some_and(|(_, id)| id == "apps"))
                && self.shell_bar_logical_origin(index).is_some_and(|origin| {
                    let size = self.window_geometry_size(index);
                    if self.frame_target_for_window(index).is_some_and(|target| {
                        self.layout_render_scale(index).is_some()
                            && self
                                .layout_editor
                                .placed
                                .iter()
                                .find(|p| p.target == target)
                                .is_some_and(|p| p.vertical)
                    }) {
                        return smithay::utils::Rectangle::new(
                            (origin.x - 5, origin.y + size.h - 59).into(),
                            (size.w + 10, 64).into(),
                        )
                        .contains(self.pointer_location.to_i32_round());
                    }
                    // Apps List is the fixed rightmost button; running applications are not drop targets.
                    smithay::utils::Rectangle::new(
                        (origin.x + size.w - 59, origin.y - 5).into(),
                        (64, size.h + 10).into(),
                    )
                    .contains(self.pointer_location.to_i32_round())
                })
        })
    }

    pub(in crate::compositor) fn desktop_pointer_motion(&mut self) -> bool {
        let Some(mut gesture) = self.desktop.gesture.take() else {
            return false;
        };
        let pointer = self.desktop_canvas_pointer();
        let consumed = match &mut gesture {
            Gesture::Move {
                id,
                start,
                positions,
                moved,
                ..
            } => {
                let delta = pointer - *start;
                *moved |= delta.x.hypot(delta.y) * self.viewport_scale >= 4.0;
                if *moved {
                    if let Some(origin) = positions.get(id) {
                        let anchor = geometry::moved_position(
                            *origin,
                            delta,
                            self.keyboard.modifier_state().alt,
                        );
                        let delta = (
                            f64::from(anchor.x - origin.x),
                            f64::from(anchor.y - origin.y),
                        )
                            .into();
                        for (id, origin) in positions.iter() {
                            if let Some(index) = self.desktop_index(id) {
                                self.windows[index].position =
                                    geometry::moved_position(*origin, delta, false);
                            }
                        }
                    }
                    self.cursor_surface = None;
                    self.cursor_icon = CursorIcon::Grabbing;
                    if id.starts_with("file:") {
                        let files: Vec<_> = positions
                            .keys()
                            .filter(|id| id.starts_with("file:"))
                            .cloned()
                            .collect();
                        self.update_desktop_drop(&files);
                    }
                    self.request_redraw();
                }
                *moved
            }
            Gesture::Marquee {
                start,
                current,
                before,
                ..
            } => {
                *current = pointer;
                let rectangle = geometry::rectangle(*start, *current);
                let mut selected = before.clone();
                for (index, window) in self.windows.iter().enumerate() {
                    if window.mapped
                        && let Some(id) = self.desktop_id(index)
                        && {
                            let mut tile = self.desktop_tile_rect(index).to_f64();
                            let offset = self.canvas_peek_offset(index);
                            tile.loc += (
                                offset.x / self.viewport_scale,
                                offset.y / self.viewport_scale,
                            )
                                .into();
                            rectangle.overlaps(tile)
                        }
                    {
                        selected.insert(id);
                    }
                }
                if self.desktop.selected != selected {
                    self.desktop.selected = selected;
                    self.desktop_changed();
                }
                self.cursor_surface = None;
                self.request_redraw();
                true
            }
        };
        self.desktop.gesture = Some(gesture);
        consumed
    }

    pub(in crate::compositor) fn desktop_edge_panning(&self) -> bool {
        if matches!(&self.desktop.gesture, Some(Gesture::Move{id,..}) if id.starts_with("app:"))
            && self.desktop_over_apps_corner()
        {
            return false;
        }
        self.desktop
            .gesture
            .as_ref()
            .is_some_and(|gesture| match gesture {
                Gesture::Move { moved, .. } => *moved,
                Gesture::Marquee { .. } => true,
            })
            && self.desktop_edge_velocity() != Point::from((0.0, 0.0))
    }

    fn desktop_edge_velocity(&self) -> Point<f64, Logical> {
        self.output_logical_rects()
            .into_iter()
            .find(|output| output.contains(self.pointer_location.to_i32_round()))
            .map(|output| geometry::edge_speed(self.pointer_location, output))
            .unwrap_or_default()
    }

    pub(in crate::compositor) fn advance_desktop_edge_pan(&mut self) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.desktop.edge_tick)
            .as_secs_f64()
            .min(0.05);
        self.desktop.edge_tick = now;
        if !self.desktop_edge_panning() {
            return;
        }
        let velocity = self.desktop_edge_velocity();
        let offset = self.viewport_offset;
        self.viewport_animation = None;
        self.viewport_offset = CanvasPoint {
            x: (f64::from(offset.x) + velocity.x * dt / self.viewport_scale)
                .round()
                .clamp(-1_000_000.0, 1_000_000.0) as i32,
            y: (f64::from(offset.y) + velocity.y * dt / self.viewport_scale)
                .round()
                .clamp(-1_000_000.0, 1_000_000.0) as i32,
        };
        self.desktop_pointer_motion();
        self.request_redraw();
    }
}
