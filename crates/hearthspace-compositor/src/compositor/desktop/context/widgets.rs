//! A context generation owns widget mutations and its original canvas location.
use crate::compositor::{App, CalloopData, layout_editor::Target, shell_integration::ControlReply};
use hearthspace_common::frame_layout::LayoutTarget;

impl App {
    pub(in crate::compositor) fn open_widget_context(
        &mut self,
        index: usize,
        keyboard: bool,
    ) -> Result<(), String> {
        let (output, id) = self
            .frame_widget_identity(index)
            .ok_or("widget disappeared")?;
        if self.widget_kind(&output, &id).is_none() || self.layout_target_on_frame(index) {
            return Err("select a canvas widget".into());
        }
        self.check_navigation_available()?;
        let bounds = self.window_screen_bounds(index);
        let anchor = if keyboard {
            bounds.loc + smithay::utils::Point::from((bounds.size.w / 2, bounds.size.h / 2))
        } else {
            self.pointer_location.to_i32_round()
        };
        self.desktop.selected.clear();
        self.begin_desktop_context(anchor, None)?;
        let context = self
            .desktop
            .context
            .current
            .as_mut()
            .expect("active context");
        context.ids = vec![format!("widget:{id}")];
        self.desktop.context.source_window = Some(self.windows[index].id);
        Ok(())
    }

    pub(super) fn widget_context_source(&self, index: usize) -> bool {
        let Some(context) = &self.desktop.context.current else {
            return false;
        };
        let Some((output, id)) = self.frame_widget_identity(index) else {
            return false;
        };
        context.output.as_deref() == Some(&output)
            && context.ids == [format!("widget:{id}")]
            && self.widget_kind(&output, &id).is_some()
            && !self.layout_target_on_frame(index)
    }
}

impl CalloopData {
    pub(super) fn run_widget_context_command(&mut self, text: &str) -> Option<ControlReply> {
        let (command, args) = text.split_once(' ')?;
        if !matches!(
            command,
            "desktop-context-widgets"
                | "desktop-context-remove-widget"
                | "desktop-context-layout-widget"
        ) {
            return None;
        }
        let result = (|| -> Result<(), String> {
            self.state.check_navigation_available()?;
            if self.state.layout_editing() {
                return Err("finish changing layout first".into());
            }
            let generation = args
                .parse::<u64>()
                .map_err(|_| "invalid context generation")?;
            let context = self
                .state
                .desktop
                .context
                .current
                .as_ref()
                .filter(|context| context.generation == generation)
                .cloned()
                .ok_or("desktop menu expired")?;
            if !self.state.desktop_context_source_available() {
                return Err("widget disappeared".into());
            }
            let output = context.output.ok_or("missing widget output")?;
            if self.state.frame_output_rect(&output).is_none() {
                return Err("widget output disappeared".into());
            }
            if command == "desktop-context-widgets" {
                if !context.ids.is_empty() {
                    return Err("add widgets from empty canvas".into());
                }
                let anchor = self.state.desktop.context.anchor;
                let (x, y) = self
                    .state
                    .desktop
                    .context
                    .canvas_anchor
                    .ok_or("missing canvas location")?;
                self.state.close_desktop_context(generation);
                return self.state.open_widget_picker(
                    hearthspace_common::canvas_search::Point {
                        x: f64::from(anchor.x),
                        y: f64::from(anchor.y),
                    },
                    output,
                    hearthspace_common::canvas_search::Point { x, y },
                );
            }
            let target = {
                let [identity] = context.ids.as_slice() else {
                    return Err("select one canvas widget".into());
                };
                let id = identity
                    .strip_prefix("widget:")
                    .ok_or("select a canvas widget")?;
                let layout = self
                    .state
                    .layout_editor
                    .layouts
                    .get_mut(&output)
                    .ok_or("widget output disappeared")?;
                let target = LayoutTarget::widget(id);
                if layout.widget_kind(id).is_none() || layout.placement(&target).is_frame() {
                    return Err("canvas widget disappeared".into());
                }
                if command == "desktop-context-remove-widget" {
                    layout.remove_widget(id)?;
                }
                target
            };
            self.state.close_desktop_context(generation);
            self.state.invalidate_shell_popups();
            if command == "desktop-context-layout-widget" {
                self.state.edit_layout_target(Target { output, target })?;
            }
            self.state.reconcile_layout_editor();
            self.state.layout_changed();
            Ok(())
        })();
        Some(result.map_or_else(ControlReply::Err, |_| ControlReply::Ok))
    }
}
