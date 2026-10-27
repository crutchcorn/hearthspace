//! One owner retirement path, preserving protocol-specific record lifetimes.
use super::{App, ManagedWindowKind, WindowDecoration, position_for_new_window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::compositor) enum RetirementCause {
    /// A null-buffer commit suspends either backend's record until remapping.
    Unmapped,
    #[cfg(feature = "xwayland")]
    X11Unmapped,
    Destroyed,
}

impl App {
    /// Buffer unmap retires presentation/input ownership while retaining the
    /// record. XWM withdrawal and destruction also retire storage and bindings.
    pub(in crate::compositor) fn retire_window(&mut self, id: u64, cause: RetirementCause) {
        let Some(index) = self.windows.index(id) else {
            return;
        };
        let window = &self.windows[index];
        let remove = cause != RetirementCause::Unmapped;
        let surface = window.surface.wl_surface();
        let restore_focus = self.focused_normal_window_id == Some(id);
        let power_confirmation = self.power_confirmation_surface(index);
        let reserved_work_area = self.reserves_shell_work_area(index);
        let restore_panel_focus = remove
            && !self.pending_search_reserves_focus()
            && self
                .active_shell_panel_index()
                .is_some_and(|index| self.windows[index].id == id);
        if power_confirmation {
            // Preserve the click before focus callbacks or surface teardown can
            // discard Smithay's implicit grab and redirect its held releases.
            self.retain_retiring_window_pointer_grab(id);
        }
        let retained_context_focus = restore_panel_focus
            && surface
                .as_ref()
                .is_some_and(|surface| self.restore_desktop_context_parent(surface));
        tracing::debug!(window_id = id, ?cause, remove, "retiring window ownership");
        self.remote_capture.retire_toplevel(id);

        // Keep Smithay's grab alive to drain every held release. Only its owner
        // is latched/cancelled; unrelated shell or popup teardown must be inert.
        self.window_pointer_grab_owner_unmapped(id);
        self.retire_window_pointer_operation(id);
        self.favorite_window_unmapped(id);
        if let Some(window) = self.windows.by_id_mut(id) {
            window.mapped = false;
            window.pending_resize = None;
        }
        if let Some(index) = self.windows.index(id) {
            if self.desktop_id(index).is_some() {
                self.desktop_mapped(index);
            }
            if !remove {
                self.suspend_window_focus_view(index);
            }
        }
        if (remove || restore_focus || power_confirmation)
            && let Some(surface) = &surface
        {
            self.clear_keyboard_focus_for_surface(surface);
        }
        if remove {
            self.idle_daemon.unregister_window(id);
            self.release_window_layout_binding(id);
            self.windows.remove_retired(id);
        }
        if self.focused_normal_window_id == Some(id) {
            self.focused_normal_window_id = None;
        }
        if reserved_work_area {
            self.reconcile_window_presentations();
        }
        if power_confirmation {
            self.finish_power_confirmation(id);
        } else if restore_panel_focus && !retained_context_focus {
            self.restore_focus_after_shell_panel();
        } else if restore_focus {
            // Automatic recovery retains viewport and any user-owned animation.
            self.restore_window_focus();
        }
        self.request_redraw();
    }

    /// Metadata changes can move a record between stack layers without replacing
    /// its identity or running destruction cleanup.
    pub(in crate::compositor) fn reclassify_window(
        &mut self,
        id: u64,
        kind: ManagedWindowKind,
        shell_output_name: Option<String>,
    ) {
        let Some(window) = self.windows.by_id_mut(id) else {
            return;
        };
        let old_kind = window.kind;
        tracing::info!(window_id = id, from = ?old_kind, to = ?kind,
            from_output = ?window.shell_output_name, to_output = ?shell_output_name,
            "toplevel app id changed window kind");
        window.kind = kind;
        window.shell_output_name = shell_output_name;
        window.position = position_for_new_window(kind, window.position);
        if kind.is_shell_chrome() || kind == ManagedWindowKind::DesktopItem {
            window.decoration = WindowDecoration::ClientSide;
        }
        if old_kind == ManagedWindowKind::Normal && kind != ManagedWindowKind::Normal {
            self.idle_daemon.unregister_window(id);
            if self.focused_normal_window_id == Some(id) {
                self.focused_normal_window_id = None;
            }
        } else if old_kind != ManagedWindowKind::Normal && kind == ManagedWindowKind::Normal {
            self.idle_daemon.register_window(id);
        }
        let before = match kind {
            ManagedWindowKind::DesktopItem => self
                .windows
                .iter()
                .find(|window| window.id != id)
                .map(|window| window.id),
            ManagedWindowKind::Normal => self.normal_stack_boundary(&[id]),
            _ => None,
        };
        self.windows.restack(&[id], before);
        self.sync_capture_toplevel(id);
    }
}
