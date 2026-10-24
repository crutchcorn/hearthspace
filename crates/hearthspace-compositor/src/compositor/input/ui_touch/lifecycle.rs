//! Revoke touch ownership without redirecting the remainder of a contact.
use super::*;

impl App {
    pub(super) fn running_app_touch_identity(&self) -> RunningAppIdentity {
        // Match the catalog shown in the dock. Stacking/focus changes leave
        // this signature unchanged; window replacement or regrouping does not.
        let mut identity: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, _)| self.is_navigable_window(*index))
            .map(|(_, window)| (window.id, window_app_id(&window.surface)))
            .collect();
        identity.sort_unstable_by_key(|(id, _)| *id);
        identity
    }

    pub(super) fn ui_touch_geometry(&self, index: usize) -> [f64; 5] {
        let projection = self.window_projection(index);
        let origin = match self.windows[index].kind {
            ManagedWindowKind::ShellBar => self.shell_bar_logical_origin(index).map(|p| p.to_f64()),
            ManagedWindowKind::Launcher => self.launcher_logical_origin(index).map(|p| p.to_f64()),
            _ => None,
        }
        .unwrap_or(projection.origin);
        let size = self.window_geometry_size(index);
        [
            origin.x,
            origin.y,
            projection.scale,
            f64::from(size.w),
            f64::from(size.h),
        ]
    }

    pub(in crate::compositor) fn cancel_ui_touch(&mut self) {
        if self.group_touch_owns_input() {
            self.finish_group_pointer_sequence(true);
        }
        self.finish_window_touch_operation();
        if let Some(touch) = self.ui_touch.as_ref() {
            self.invalidate_gtk_touch_window(touch.window);
        }
        if let Some(touch) = self.ui_touch.as_mut()
            && touch.cancel()
        {
            self.touch.clone().cancel(self);
        }
    }

    pub(in crate::compositor) fn maintain_ui_touch(&mut self) {
        // Group movement changes its members' geometry by design. Ownership
        // lasts until release/cancel even if the group is cleared meanwhile.
        if self.group_touch_suppresses_gestures() {
            return;
        }
        if self.maintain_window_touch_operation() || self.maintain_frame_scroll_ui_touch() {
            return;
        }
        if let Some(touch) = self.ui_touch.as_ref() {
            // Bar taps become a pointer press/release only after touch-up. A
            // fixed-width dock can replace a tile meanwhile without changing
            // its surface geometry; never deliver that tap to the replacement.
            let apps_identity = touch
                .apps_identity
                .as_ref()
                .map(|_| self.running_app_touch_identity());
            let geometry = self
                .windows
                .iter()
                .position(|window| window.id == touch.window && window.mapped)
                .map(|index| self.ui_touch_geometry(index));
            let alive = match &touch.target {
                Target::Client(client) => client.surface.is_alive(),
                _ => true,
            };
            let valid = alive
                && touch.valid(
                    geometry,
                    &self.output_logical_rects(),
                    self.navigation_available()
                        || self.canvas_search_active()
                        || self.layout_editing()
                        || self.layout_confirmation_active(),
                    apps_identity.as_deref(),
                );
            if !valid {
                self.cancel_ui_touch();
            }
        }
    }

    pub(in crate::compositor) fn ui_touch_focus_changed(
        &mut self,
        window: Option<u64>,
        surface: Option<&WlSurface>,
    ) {
        // Called under Smithay's keyboard lock; do not query keyboard state here.
        self.gtk_touch_focus_changed(surface);
        if self.ui_touch.as_ref().is_some_and(|touch| {
            !matches!(touch.target, Target::Group | Target::GroupCancelled)
                && Some(touch.window) != window
        }) {
            self.cancel_ui_touch();
        }
    }

    pub(in crate::compositor::input) fn suppress_ui_touch_pointer(
        &self,
        device: &str,
        action: &TouchAction,
    ) -> bool {
        self.ui_touch
            .as_ref()
            .is_some_and(|touch| touch.suppresses_pointer(device, action))
    }

    pub(in crate::compositor::input) fn finish_ui_touch_frame(&mut self) {
        self.touch.clone().frame(self);
        if !self.touch_input.has_contacts() && !self.window_touch_has_pointer_buttons() {
            let operation = self.has_window_touch_operation();
            self.ui_touch = None;
            if operation {
                self.apply_pointer_motion(self.pointer_location, InputTime::now());
            }
        }
    }
}
