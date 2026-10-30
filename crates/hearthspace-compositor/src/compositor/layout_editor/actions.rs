use super::{App, FocusRequest, LayoutTarget, ObjectPlacement, Session, Target, TargetKind};
use hearthspace_common::frame_layout::{self, FrameEdge};
use hearthspace_ipc::frame_layout::{Action, Command};

impl App {
    pub(in crate::compositor) fn open_layout_editor(&mut self) -> Result<(), String> {
        if self.layout_editing() {
            return Ok(());
        }
        self.check_canvas_tool_available()?;
        self.disarm_canvas_pan();
        self.disarm_canvas_search();
        self.leave_window_focus_views();
        self.cancel_desktop_gesture();
        self.clear_window_group();
        let pointer = self.pointer.current_location();
        let outputs = self.capture_outputs();
        let output = outputs
            .iter()
            .find(|o| {
                pointer.x >= f64::from(o.logical_x)
                    && pointer.x < f64::from(o.logical_x + o.logical_width)
                    && pointer.y >= f64::from(o.logical_y)
                    && pointer.y < f64::from(o.logical_y + o.logical_height)
            })
            .or_else(|| outputs.first())
            .ok_or("no active output")?
            .name
            .clone();
        self.invalidate_shell_popups();
        self.layout_editor.sequence = self.layout_editor.sequence.wrapping_add(1).max(1);
        self.layout_editor.session = Some(Session {
            id: self.layout_editor.sequence,
            editing: true,
            started: std::time::Instant::now(),
            output,
            selected: None,
            previous_focus: self.focused_normal_window_id,
            confirmation: None,
            confirmation_focus: None,
        });
        self.layout_editor.expanded_window = None;
        self.layout_editor.hovered_window = None;
        self.clear_keyboard_focus();
        self.layout_changed();
        Ok(())
    }
    pub(in crate::compositor) fn reset_layout_editor(&mut self) {
        self.cancel_layout_drag();
        self.layout_editor.session = None;
        self.layout_editor.focus_request = None;
        self.layout_editor.expanded_window = None;
        self.layout_editor.hovered_window = None;
        self.clear_layout_motion();
        self.clear_layout_turn();
        self.layout_changed();
    }
    pub(super) fn check_layout_target(&self, target: &Target) -> Result<(), String> {
        if !target.target.valid() || !self.layout_editor.layouts.contains_key(&target.output) {
            return Err("invalid layout target".into());
        }
        if target.target.kind == TargetKind::Window && target.target.id.parse::<u64>().is_err() {
            return Err("window commands require a live window ID".into());
        }
        if matches!(target.target.kind, TargetKind::Window | TargetKind::Item)
            && self.target_window_index(target).is_none()
        {
            return Err("layout target disappeared".into());
        }
        if target.target.kind == TargetKind::Group
            && !self.layout_editor.layouts[&target.output]
                .groups
                .contains_key(&target.target.id)
        {
            return Err("frame group disappeared".into());
        }
        Ok(())
    }
    pub(super) fn apply_layout_command(&mut self, command: Command) -> Result<(), String> {
        if !self
            .layout_editor
            .session
            .as_ref()
            .is_some_and(|s| s.id == command.session)
        {
            return Err("layout editing session expired".into());
        }
        if matches!(command.action, Action::Cancel) {
            self.cancel_layout_confirmation();
            return Ok(());
        }
        if matches!(command.action, Action::Confirm) {
            return self.confirm_layout_action();
        }
        if self.layout_confirmation_active() {
            return Err("respond to the layout confirmation first".into());
        }
        if !self.layout_editing() {
            return Err("layout editor is not open".into());
        }
        if matches!(command.action, Action::Done) {
            self.finish_layout_editor();
            return Ok(());
        }
        if let Action::AllowHideTools { value } = command.action {
            self.apply_frame_settings(Some(value), false);
            return Ok(());
        }
        let target = Target {
            output: command.output,
            target: command
                .action
                .target()
                .cloned()
                .ok_or("missing layout target")?,
        };
        self.check_layout_target(&target)?;
        match command.action {
            Action::Select { .. } => {
                self.layout_editor.session.as_mut().unwrap().selected = Some(target)
            }
            Action::Place { placement, .. } => {
                if !placement.valid() {
                    return Err("invalid placement".into());
                }
                if target.target.kind == TargetKind::Group
                    && matches!(
                        placement,
                        ObjectPlacement::Canvas { .. } | ObjectPlacement::Background { .. }
                    )
                {
                    self.release_layout_group(
                        &target,
                        matches!(placement, ObjectPlacement::Background { .. }),
                    );
                    self.layout_editor.session.as_mut().unwrap().selected = None;
                } else {
                    self.place_layout_target(&target, placement);
                }
            }
            Action::Attach { .. } => {
                let view = self
                    .frame_output_rect(&target.output)
                    .ok_or("missing output")?;
                let mut rect = self
                    .frame_placed(&target)
                    .map(|p| p.rect)
                    .or_else(|| {
                        self.target_window_index(&target).map(|i| {
                            let r = self.window_screen_bounds(i);
                            super::Rect {
                                x: f64::from(r.loc.x),
                                y: f64::from(r.loc.y),
                                width: f64::from(r.size.w),
                                height: f64::from(r.size.h),
                            }
                        })
                    })
                    .ok_or("missing target geometry")?;
                rect.x -= view.x;
                rect.y -= view.y;
                let placement = frame_layout::frame_drop_target(rect, view, f64::MAX, None)
                    .unwrap_or(ObjectPlacement::frame(FrameEdge::Top, 0.5));
                self.place_layout_target(&target, placement);
            }
            Action::Remove { .. } => {
                if self.layout_removal_hides_protected_tools(&target) {
                    self.request_tools_confirmation();
                } else {
                    let placement = if target.target.kind == TargetKind::Widget
                        || target.target.kind == TargetKind::Group
                    {
                        ObjectPlacement::Hidden
                    } else {
                        frame_layout::default_placement(&target.target)
                    };
                    self.place_layout_target(&target, placement);
                    self.layout_editor.session.as_mut().unwrap().selected = None;
                }
            }
            Action::Ungroup { .. } => self.separate_frame_target(&target),
            Action::ToggleExpanded { .. } => {
                self.layout_editor
                    .layouts
                    .get_mut(&target.output)
                    .unwrap()
                    .toggle_expanded(&target.target.id);
            }
            Action::ClockDateFormat { value: format, .. } => {
                self.layout_editor
                    .layouts
                    .get_mut(&target.output)
                    .unwrap()
                    .clock_date_format = format;
            }
            Action::Done | Action::Cancel | Action::Confirm | Action::AllowHideTools { .. } => {
                unreachable!("handled before selecting a target")
            }
        }
        self.reconcile_layout_editor();
        self.layout_changed();
        Ok(())
    }
    pub(super) fn apply_frame_settings(&mut self, allow: Option<bool>, restore: bool) {
        if let Some(allow) = allow {
            self.layout_editor.allow_hide_tools = allow;
            for layout in self.layout_editor.layouts.values_mut() {
                layout.set_allow_hide_tools(allow);
            }
        }
        if restore {
            for layout in self.layout_editor.layouts.values_mut() {
                let target = LayoutTarget::widget("tools");
                layout.set_placement(&target, frame_layout::default_placement(&target));
            }
        }
        self.reconcile_layout_editor();
        self.layout_changed();
    }
    pub(in crate::compositor) fn finish_layout_editor(&mut self) {
        self.cancel_layout_drag();
        let focus = self
            .layout_editor
            .session
            .take()
            .and_then(|s| s.previous_focus);
        self.layout_changed();
        if let Some(id) = focus
            && let Some(index) = self.windows.iter().position(|w| w.id == id && w.mapped)
            && let Some(surface) = self.windows[index].surface.wl_surface()
        {
            self.set_keyboard_focus_to_window(index, surface);
        }
    }
    pub(in crate::compositor) fn toggle_frame_window_reveal(
        &mut self,
        id: u64,
    ) -> Result<bool, String> {
        if self.pinned_window_target(id).is_none() {
            return Ok(false);
        }
        if self.layout_editor.expanded_window != Some(id) {
            self.reveal_frame_window(id)?;
            return Ok(true);
        }
        let index = self
            .windows
            .iter()
            .position(|window| window.id == id && window.mapped)
            .ok_or("pinned window disappeared")?;
        let from = self
            .layout_motion_rect(index)
            .or_else(|| self.layout_target_rect(index))
            .ok_or("pinned window has no placement")?;
        self.layout_editor.expanded_window = None;
        if let Some(to) = self.layout_target_rect(index) {
            self.queue_layout_motion(id, from, to, super::motion::REVEAL_DURATION_MS);
        }
        // A tucked client cannot receive ordinary keyboard input. Its native
        // reveal control is the keyboard entry point for opening it again.
        let reveal = self.windows.iter().position(|window| {
            window.mapped
                && super::window_app_id(&window.surface)
                    .as_deref()
                    .and_then(frame_layout::pinned_window_reveal_from_app_id)
                    == Some(id)
        });
        if let Some(reveal) = reveal
            && let Some(surface) = self.windows[reveal].surface.wl_surface()
        {
            self.set_keyboard_focus_to_window(reveal, surface);
        } else {
            self.clear_keyboard_focus();
        }
        self.layout_changed();
        Ok(true)
    }

    pub(in crate::compositor) fn reveal_frame_window(&mut self, id: u64) -> Result<(), String> {
        let index = self
            .windows
            .iter()
            .position(|w| w.id == id && w.mapped)
            .ok_or("pinned window disappeared")?;
        if self.pinned_window_target(id).is_none() {
            return Err("window is not pinned".into());
        }
        let from = self
            .layout_motion_rect(index)
            .or_else(|| self.layout_target_rect(index))
            .unwrap();
        self.layout_editor.expanded_window = Some(id);
        let to = self.layout_target_rect(index).unwrap();
        self.queue_layout_motion(id, from, to, super::motion::REVEAL_DURATION_MS);
        if let Some(surface) = self.windows[index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(index, surface);
        }
        self.layout_changed();
        Ok(())
    }
    pub(in crate::compositor) fn focus_next_frame_control(
        &mut self,
        target: Target,
        reverse: bool,
    ) -> Result<(), String> {
        self.check_layout_target(&target)?;
        let controls: Vec<_> = self
            .layout_editor
            .placed
            .iter()
            .filter(|p| {
                p.target.output == target.output
                    && p.target.target.kind != TargetKind::Group
                    && (p.target.target.kind == TargetKind::Widget || p.placement.is_frame())
                    && (p.target.target.kind != TargetKind::Window
                        || p.target.target.id.parse::<u64>().ok()
                            != self.layout_editor.expanded_window)
            })
            .filter_map(|p| {
                let index = if p.target.target.kind == TargetKind::Window {
                    let id = p.target.target.id.parse::<u64>().ok()?;
                    self.windows.iter().position(|window| {
                        window.mapped
                            && super::window_app_id(&window.surface)
                                .as_deref()
                                .and_then(frame_layout::pinned_window_reveal_from_app_id)
                                == Some(id)
                    })
                } else {
                    self.target_window_index(&p.target)
                }?;
                Some((p.target.clone(), index))
            })
            .collect();
        if controls.is_empty() {
            return Err("no visible frame controls".into());
        }
        let current = controls
            .iter()
            .position(|(item, _)| *item == target)
            .unwrap_or(0);
        let next = if reverse {
            (current + controls.len() - 1) % controls.len()
        } else {
            (current + 1) % controls.len()
        };
        let (target, index) = &controls[next];
        if let Some(surface) = self.windows[*index].surface.wl_surface() {
            self.set_keyboard_focus_to_window(*index, surface);
        }
        self.layout_editor.focus_serial = self.layout_editor.focus_serial.wrapping_add(1).max(1);
        self.layout_editor.focus_request = Some(FocusRequest {
            serial: self.layout_editor.focus_serial,
            output: target.output.clone(),
            target: target.target.clone(),
            reverse,
        });
        self.layout_changed();
        Ok(())
    }
}
