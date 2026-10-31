//! Generation-scoped commands share the compositor-owned menu lifetime.
use crate::compositor::{CalloopData, shell_integration::ControlReply};

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
        if !app.desktop_context_source_available()
            || context.ids.iter().any(|id| {
                if app.desktop.context.source_window.is_some() {
                    !app.app_context_item_available(id)
                } else {
                    app.desktop_index(id).is_none()
                }
            })
        {
            return Some(ControlReply::Err(
                "Desktop menu item is no longer available".into(),
            ));
        }
        if matches!(command, "desktop-context-parent" | "desktop-context-child") {
            app.focus_desktop_context_menu(generation, command == "desktop-context-child");
            return Some(ControlReply::Ok);
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
            return Some(app.open_background_settings_reply(context.output.as_deref()));
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
