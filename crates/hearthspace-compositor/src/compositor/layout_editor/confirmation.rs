//! Dialogs are modal independently of whether the frame editor is open.
use hearthspace_common::frame_layout::{LayoutTarget, TargetKind, layout_confirm_from_app_id};

use super::{App, Session, Target, window_app_id};

pub(super) use hearthspace_ipc::frame_layout::Confirmation;

impl App {
    pub(in crate::compositor) fn layout_confirmation_active(&self) -> bool {
        self.layout_editor
            .session
            .as_ref()
            .is_some_and(|session| session.confirmation.is_some())
    }

    pub(in crate::compositor) fn layout_confirmation_surface(&self, index: usize) -> bool {
        let Some((id, output)) = window_app_id(&self.windows[index].surface)
            .as_deref()
            .and_then(layout_confirm_from_app_id)
        else {
            return false;
        };
        self.layout_editor.session.as_ref().is_some_and(|session| {
            session.id == id
                && session.confirmation.is_some()
                && output.as_deref().unwrap_or(&session.output) == session.output
        })
    }

    pub(super) fn layout_removal_hides_protected_tools(&self, target: &Target) -> bool {
        if self.layout_editor.allow_hide_tools {
            return false;
        }
        if target.target == LayoutTarget::widget("tools") {
            return true;
        }
        target.target.kind == TargetKind::Group
            && self
                .layout_editor
                .layouts
                .get(&target.output)
                .and_then(|layout| layout.groups.get(&target.target.id))
                .is_some_and(|group| group.members.contains(&LayoutTarget::widget("tools")))
    }

    fn layout_current_focus_id(&self) -> Option<u64> {
        let focus = self.keyboard.current_focus()?;
        self.window_index_for_surface(focus.surface())
            .map(|index| self.windows[index].id)
    }

    pub(super) fn request_tools_confirmation(&mut self) {
        if self.layout_confirmation_active() {
            return;
        }
        self.cancel_layout_drag();
        let focus = self.layout_current_focus_id();
        if let Some(session) = &mut self.layout_editor.session {
            session.confirmation = Some(Confirmation::HideTools);
            session.confirmation_focus = focus;
            self.invalidate_shell_popups();
        }
        self.clear_keyboard_focus();
        self.layout_changed();
    }

    /// Returns true when a pinned close has been intercepted, including repeated
    /// requests while its confirmation is already awaiting a response.
    pub(in crate::compositor) fn request_pinned_window_close(&mut self, id: u64) -> bool {
        let Some(target) = self
            .pinned_window_target(id)
            .map(|placed| placed.target.clone())
        else {
            return false;
        };
        if self.layout_confirmation_active() || self.session_locked() {
            return true;
        }
        self.cancel_layout_drag();
        let focus = self.layout_current_focus_id();
        if self.layout_editor.session.is_none() {
            self.layout_editor.sequence = self.layout_editor.sequence.wrapping_add(1).max(1);
            self.layout_editor.session = Some(Session {
                id: self.layout_editor.sequence,
                editing: false,
                started: std::time::Instant::now(),
                output: target.output,
                selected: None,
                previous_focus: self.focused_normal_window_id,
                confirmation: None,
                confirmation_focus: None,
            });
        }
        let session = self.layout_editor.session.as_mut().unwrap();
        session.confirmation = Some(Confirmation::CloseWindow { window_id: id });
        session.confirmation_focus = focus;
        self.invalidate_shell_popups();
        self.clear_keyboard_focus();
        self.layout_changed();
        true
    }

    pub(in crate::compositor) fn cancel_layout_confirmation(&mut self) {
        let Some(session) = &mut self.layout_editor.session else {
            return;
        };
        if session.confirmation.take().is_none() {
            return;
        }
        let focus = session.confirmation_focus.take().or(session.previous_focus);
        if !session.editing {
            self.layout_editor.session = None;
        }
        self.layout_changed();
        if let Some(index) =
            focus.and_then(|id| self.windows.iter().position(|w| w.id == id && w.mapped))
            && let Some(surface) = self.windows[index].surface.wl_surface()
        {
            self.set_keyboard_focus_to_window(index, surface);
        }
    }

    pub(super) fn confirm_layout_action(&mut self) -> Result<(), String> {
        let session = self
            .layout_editor
            .session
            .as_ref()
            .ok_or("layout session expired")?;
        let confirmation = session
            .confirmation
            .ok_or("no pending layout confirmation")?;
        let output = session.output.clone();
        match confirmation {
            Confirmation::HideTools => {
                // This is an explanation with a Settings link, never permission
                // to silently enable hiding or remove the tools.
                self.finish_layout_editor();
                self.open_layout_settings(Some(&output))?;
            }
            Confirmation::CloseWindow { window_id } => {
                let index = self
                    .windows
                    .iter()
                    .position(|w| w.id == window_id && w.mapped)
                    .ok_or("pinned window disappeared")?;
                if self.pinned_window_target(window_id).is_none() {
                    self.cancel_layout_confirmation();
                    return Err("window is no longer pinned".into());
                }
                let target = LayoutTarget::new(TargetKind::Window, window_id.to_string());
                let canonical = self.canonical_layout_target(&target);
                for layout in self.layout_editor.layouts.values_mut() {
                    layout.remove_target(&canonical);
                }
                if self.layout_editor.expanded_window == Some(window_id) {
                    self.layout_editor.expanded_window = None;
                }
                if self.layout_editor.hovered_window == Some(window_id) {
                    self.layout_editor.hovered_window = None;
                }
                if let Some(session) = &mut self.layout_editor.session
                    && session
                        .selected
                        .as_ref()
                        .is_some_and(|selected| selected.target == target)
                {
                    session.selected = None;
                }
                // Removing just this target preserves surviving group members.
                self.layout_editor.placed = self.frame_placements();
                self.windows[index].surface.send_close();
                self.cancel_layout_confirmation();
            }
        }
        self.layout_changed();
        Ok(())
    }

    pub(super) fn reconcile_layout_confirmation(&mut self) {
        let Some(session) = self.layout_editor.session.as_ref() else {
            return;
        };
        if self.frame_output_rect(&session.output).is_none() {
            self.finish_layout_editor();
            return;
        }
        if let Some(Confirmation::CloseWindow { window_id }) = session.confirmation
            && (!self
                .windows
                .iter()
                .any(|window| window.id == window_id && window.mapped)
                || self.pinned_window_target(window_id).is_none())
        {
            self.cancel_layout_confirmation();
        }
    }
}
