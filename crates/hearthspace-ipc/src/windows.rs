//! Window metadata produced by the compositor and projected by shell clients.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowList {
    pub version: u32,
    pub windows: Vec<Window>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Window {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub titlebar_height: i32,
    pub canvas_x: i32,
    pub canvas_y: i32,
    pub content_width: i32,
    pub content_height: i32,
    pub mapped: bool,
    pub visible: bool,
    pub outputs: Vec<String>,
    pub focused: bool,
    pub client_type: String,
    pub fullscreen: bool,
    pub maximized: bool,
    pub focused_view: bool,
    pub maximized_canvas_bounds: Option<Bounds>,
    pub output_coordinates: bool,
    pub presentation_output: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WindowList {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let list: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("Invalid window list: {error}"))?;
        if list.version != 1 {
            return Err("Unsupported window list version".into());
        }
        if list.windows.iter().any(|window| window.id == 0) {
            return Err("Invalid window ID".into());
        }
        Ok(list)
    }
}
