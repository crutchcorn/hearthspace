use hearthspace_common::frame_layout::{FrameLayout, LayoutTarget};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Widget {
    pub id: String,
    pub group: Option<String>,
    pub orientation: String,
    pub rect: Rect,
    pub expanded: bool,
    pub merged: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    pub id: String,
    pub rect: Rect,
    pub grip: Rect,
    #[serde(default = "group_scale")]
    pub scale: f64,
    pub orientation: String,
    pub hovered: bool,
    pub focused: bool,
    #[serde(default = "group_material")]
    pub material: bool,
    pub backing_only: bool,
}

fn group_scale() -> f64 {
    1.0
}

fn group_material() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Output {
    pub output: String,
    pub rect: Rect,
    pub compact: bool,
    pub layout: FrameLayout,
    pub widgets: Vec<Widget>,
    pub groups: Vec<Group>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    pub output: String,
    pub target: LayoutTarget,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FocusRequest {
    pub serial: u64,
    pub output: String,
    pub target: LayoutTarget,
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PinnedWindow {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub output: String,
    pub rect: Rect,
    pub expanded: bool,
    pub hovered: bool,
    pub edge: Option<hearthspace_common::frame_layout::FrameEdge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Confirmation {
    HideTools,
    CloseWindow { window_id: u64 },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub version: u32,
    #[serde(default)]
    pub canvas_tool: Option<crate::command::CanvasTool>,
    pub animating: bool,
    pub allow_hide_tools: bool,
    pub revision: u64,
    pub popup_epoch: u64,
    pub editing: bool,
    pub session: u64,
    pub output: Option<String>,
    pub selected: Option<Selection>,
    pub selected_title: String,
    pub confirmation: Option<Confirmation>,
    #[serde(default = "confirm_width")]
    pub confirmation_max_width: f64,
    #[serde(default = "editor_height")]
    pub confirmation_max_height: f64,
    pub outputs: Vec<Output>,
    pub pinned_windows: Vec<PinnedWindow>,
    pub focus_request: Option<FocusRequest>,
    #[serde(default = "editor_width")]
    pub editor_max_width: f64,
    #[serde(default = "editor_height")]
    pub editor_max_height: f64,
}

fn editor_width() -> f64 {
    260.0
}
fn editor_height() -> f64 {
    452.0
}
fn confirm_width() -> f64 {
    420.0
}

impl From<hearthspace_common::frame_layout::Rect> for Rect {
    fn from(rect: hearthspace_common::frame_layout::Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}
