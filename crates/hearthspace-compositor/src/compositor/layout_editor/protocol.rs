//! Session guarded commands keep stale native editor surfaces inert.
use super::Target;
use crate::compositor::{CalloopData, shell_integration::ControlReply};

use hearthspace_ipc::frame_layout::{
    Anchor, Available, ClockFormat, Command, Focus, FocusRegion, Settings,
};

impl CalloopData {
    pub(in crate::compositor) fn run_layout_command(&mut self, text: &str) -> Option<ControlReply> {
        if !text.starts_with("layout-") && !text.starts_with("frame-") {
            return None;
        }
        let available = self.running && self.capture_active() && !self.state.session_locked();
        if text == "layout-status" {
            return Some(ControlReply::Payload(
                serde_json::to_vec(&self.state.layout_status(available)).unwrap(),
            ));
        }
        if text == "frame-settings-status" {
            return Some(ControlReply::Payload(
                serde_json::to_vec(&hearthspace_ipc::frame_layout::SettingsSnapshot {
                    version: 1,
                    allow_hide_tools: self.state.layout_editor.allow_hide_tools,
                })
                .unwrap(),
            ));
        }
        if !available {
            return Some(ControlReply::Err("layout editing is inactive".into()));
        }
        let result = (|| -> Result<(), String> {
            if let Some(value) = text.strip_prefix("frame-clock-format ") {
                self.state.check_navigation_available()?;
                let value: ClockFormat =
                    serde_json::from_str(value).map_err(|_| "invalid clock format")?;
                if self.state.frame_output_rect(&value.output).is_none() {
                    return Err("unknown clock output".into());
                }
                let layout = self
                    .state
                    .layout_editor
                    .layouts
                    .get_mut(&value.output)
                    .ok_or("unknown clock output")?;
                if layout.clock_date_format != value.value {
                    layout.clock_date_format = value.value;
                    self.state.reconcile_layout_editor();
                    self.state.layout_changed();
                }
                return Ok(());
            }
            if let Some(value) = text.strip_prefix("frame-focus-region ") {
                let value: FocusRegion =
                    serde_json::from_str(value).map_err(|_| "invalid frame focus region")?;
                let rect = value.rect;
                if ![rect.x, rect.y, rect.width, rect.height]
                    .into_iter()
                    .all(|v| v.is_finite() && v.abs() <= 100_000.)
                    || rect.width <= 0.
                    || rect.height <= 0.
                {
                    return Err("invalid frame focus region".into());
                }
                let target = Target {
                    output: value.output,
                    target: value.target,
                };
                self.state.check_layout_target(&target)?;
                if let Some(index) = self.state.target_window_index(&target)
                    && self.state.keyboard.current_focus().is_some_and(|focus| {
                        self.state.window_index_for_surface(focus.surface()) == Some(index)
                    })
                {
                    self.state.reveal_layout_member(
                        index,
                        Some(super::Rect {
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                        }),
                    );
                }
                return Ok(());
            }
            if let Some(value) = text.strip_prefix("frame-widget-available ") {
                let value: Available =
                    serde_json::from_str(value).map_err(|_| "invalid frame availability")?;
                return self.state.set_frame_widget_available(
                    value.output,
                    value.id,
                    value.available,
                );
            }
            if text == "layout-tool" {
                return self.state.open_layout_editor();
            }
            if let Some(value) = text.strip_prefix("layout-command ") {
                let command: Command =
                    serde_json::from_str(value).map_err(|_| "invalid layout command")?;
                return self.state.apply_layout_command(command);
            }
            if let Some(value) = text.strip_prefix("frame-settings ") {
                let value: Settings =
                    serde_json::from_str(value).map_err(|_| "invalid frame settings")?;
                self.state.apply_frame_settings(
                    value.allow_hide_tools,
                    value.restore_tools.unwrap_or(false),
                );
                return Ok(());
            }
            if let Some(value) = text.strip_prefix("frame-reveal ") {
                self.state.check_navigation_available()?;
                return self
                    .state
                    .reveal_frame_window(value.parse().map_err(|_| "invalid pinned window")?);
            }
            if let Some(value) = text.strip_prefix("frame-focus-next ") {
                self.state.check_navigation_available()?;
                let value: Focus =
                    serde_json::from_str(value).map_err(|_| "invalid frame focus")?;
                return self.state.focus_next_frame_control(
                    Target {
                        output: value.output,
                        target: value.target,
                    },
                    value.reverse,
                );
            }
            if let Some(value) = text.strip_prefix("frame-popup-anchor ") {
                let value: Anchor =
                    serde_json::from_str(value).map_err(|_| "invalid frame popup anchor")?;
                if value.app_id.is_empty()
                    || value.app_id.len() > 4096
                    || hearthspace_common::shell_panel_output_name_from_app_id(&value.app_id)
                        .is_none()
                {
                    return Err("invalid popup surface".into());
                }
                let target = Target {
                    output: value.output,
                    target: value.target,
                };
                self.state.check_layout_target(&target)?;
                self.state
                    .layout_editor
                    .popup_anchors
                    .insert(value.app_id, target);
                return Ok(());
            }
            Err("invalid layout command".into())
        })();
        Some(match result {
            Ok(()) => ControlReply::Ok,
            Err(error) => ControlReply::Err(error),
        })
    }
}
