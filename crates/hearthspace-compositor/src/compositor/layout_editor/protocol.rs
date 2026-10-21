//! Session guarded commands keep stale native editor surfaces inert.
use super::geometry::{grip_rect, json_rect};
use super::{App, LayoutTarget, ObjectPlacement, Target};
use crate::compositor::{CalloopData, shell_integration::ControlReply};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Command {
    pub session: u64,
    pub output: String,
    pub action: String,
    pub target: Option<LayoutTarget>,
    pub placement: Option<ObjectPlacement>,
    pub value: Option<Value>,
    #[serde(rename = "reverse")]
    pub _reverse: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    allow_hide_tools: Option<bool>,
    restore_tools: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Focus {
    output: String,
    target: LayoutTarget,
    #[serde(default)]
    reverse: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Anchor {
    app_id: String,
    output: String,
    target: LayoutTarget,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusRegion {
    output: String,
    target: LayoutTarget,
    rect: FocusRect,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl App {
    fn layout_status(&self, available: bool) -> Value {
        let state = &self.layout_editor;
        if !available {
            return json!({"version":1,"animating":available&&self.layout_animating(),"revision":state.revision,"editing":false,"session":0,"output":null,"selected":null,"selected_title":"","allow_hide_tools":state.allow_hide_tools,"outputs":[],"pinned_windows":[],"confirmation":null,"focus_request":null});
        }
        let outputs:Vec<_>=state.layouts.iter().filter(|(name,_)|self.frame_output_rect(name).is_some()).map(|(output,layout)|{
            let widgets:Vec<_>=hearthspace_common::frame_layout::WIDGETS.iter().map(|id|{
                let target=Target{output:output.clone(),target:LayoutTarget::widget(*id)};
                let placed=self.frame_placed(&target);
                json!({"id":id,"merged":self.frame_widget_merged(output,id),"group":placed.and_then(|p|p.group.as_deref()),"orientation":if placed.is_some_and(|p|p.vertical){"vertical"}else{"horizontal"},"rect":json_rect(placed.map_or(Default::default(),|p|p.rect)),"expanded":layout.is_expanded(id),"hidden":layout.placement(&target.target)==ObjectPlacement::Hidden})
            }).collect();
            let groups:Vec<_>=state.placed.iter().filter(|p|p.target.output==*output&&p.target.target.kind==super::TargetKind::Group).map(|p|json!({"id":p.target.target.id,"material":self.frame_group_material(output,&p.target.target.id),"backing_only":super::compact::backing(&p.target.target.id),"hovered":self.frame_group_hovered(output,&p.target.target.id),"focused":self.frame_group_focused(output,&p.target.target.id),"rect":json_rect(p.rect),"grip":json_rect(grip_rect(p.rect,p.placement.edge())),"orientation":if p.vertical{"vertical"}else{"horizontal"}})).collect();
            json!({"output":output,"compact":self.frame_output_rect(output).is_some_and(|view|view.width<=760.),"layout":layout,"widgets":widgets,"groups":groups})
        }).collect();
        let pinned:Vec<_>=self.windows.iter().filter_map(|w|{
            let p=self.pinned_window_target(w.id)?;
            Some(json!({"id":w.id,"title":super::window_title(&w.surface).unwrap_or_default(),"app_id":super::window_app_id(&w.surface).unwrap_or_default(),"edge":p.placement.edge(),"output":p.target.output,"rect":json_rect(self.pinned_reveal_rect(p)),"expanded":state.expanded_window==Some(w.id),"hovered":state.hovered_window==Some(w.id)}))
        }).collect();
        let session = state.session.as_ref();
        let selected = session.and_then(|s| s.selected.as_ref());
        let view = session
            .and_then(|s| self.frame_output_rect(&s.output))
            .unwrap_or_default();
        json!({"version":1,"animating":available&&self.layout_animating(),"revision":state.revision,"editing":session.is_some_and(|s|s.editing),"session":session.map_or(0,|s|s.id),"output":session.map(|s|s.output.as_str()),"selected":selected,"selected_title":selected.map_or_else(String::new,|t|self.target_title(t)),"allow_hide_tools":state.allow_hide_tools,"outputs":outputs,"pinned_windows":pinned,"confirmation":session.and_then(|s|s.confirmation.as_ref()),"focus_request":state.focus_request,"confirmation_max_width":(view.width-48.).clamp(1.,420.),"confirmation_max_height":(view.height-48.).max(1.),"editor_max_width":(view.width-24.).clamp(120.,260.),"editor_max_height":(view.height-if view.width<=760.{156.}else{180.}).max(54.)})
    }
}
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
            return Some(ControlReply::Payload(serde_json::to_vec(&json!({"version":1,"allow_hide_tools":self.state.layout_editor.allow_hide_tools})).unwrap()));
        }
        if !available {
            return Some(ControlReply::Err("layout editing is inactive".into()));
        }
        let result = (|| -> Result<(), String> {
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
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Available {
                    output: String,
                    id: String,
                    available: bool,
                }
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
