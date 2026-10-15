use serde_json::Value;
use xilem::kurbo::Rect;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct CanvasSnapshot {
    pub(super) offset: (f64, f64),
    pub(super) scale: f64,
    pub(super) outputs: Vec<MapOutput>,
    pub(super) windows: Vec<MapWindow>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct MapOutput {
    pub(super) name: String,
    pub(super) bounds: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct MapWindow {
    pub(super) id: u64,
    pub(super) title: String,
    pub(super) app_id: String,
    pub(super) bounds: Rect,
    pub(super) focused: bool,
    pub(super) output_coordinates: bool,
}

impl CanvasSnapshot {
    pub(in crate::ui) fn scale(&self) -> f64 {
        self.scale
    }

    pub(in crate::ui) fn set_offset(&mut self, x: i32, y: i32) {
        let next = (f64::from(x), f64::from(y));
        let delta = xilem::kurbo::Vec2::new(next.0 - self.offset.0, next.1 - self.offset.1);
        // Output-pinned windows and their dialogs stay attached to the visible
        // output even while live status polling is paused during a map drag.
        for window in &mut self.windows {
            if window.output_coordinates {
                window.bounds = window.bounds + delta;
            }
        }
        self.offset = next;
    }

    pub(super) fn output_bounds(&self, output: &MapOutput) -> Rect {
        self.screen_to_canvas(output.bounds)
    }

    fn screen_to_canvas(&self, rect: Rect) -> Rect {
        Rect::new(
            rect.x0 / self.scale + self.offset.0,
            rect.y0 / self.scale + self.offset.1,
            rect.x1 / self.scale + self.offset.0,
            rect.y1 / self.scale + self.offset.1,
        )
    }
}

pub(in crate::ui) fn parse_snapshot(bytes: &[u8]) -> Result<CanvasSnapshot, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid canvas status: {error}"))?;
    let viewport = value.get("viewport").ok_or("Missing canvas viewport")?;
    let scale = number(viewport, "scale")?;
    if !(1e-6..=1e6).contains(&scale) {
        return Err("Invalid canvas scale".into());
    }
    let mut snapshot = CanvasSnapshot {
        offset: (number(viewport, "x")?, number(viewport, "y")?),
        scale,
        outputs: Vec::new(),
        windows: Vec::new(),
    };
    if snapshot.offset.0.abs() > 1e12 || snapshot.offset.1.abs() > 1e12 {
        return Err("Invalid canvas offset".into());
    }
    for output in array(&value, "outputs")? {
        let name = output["name"].as_str().ok_or("Missing output name")?;
        let bounds = rect(
            number(output, "logical_x")?,
            number(output, "logical_y")?,
            number(output, "logical_width")?,
            number(output, "logical_height")?,
        )?;
        validate_rect(snapshot.screen_to_canvas(bounds))?;
        snapshot.outputs.push(MapOutput {
            name: name.to_string(),
            bounds,
        });
    }
    for window in array(&value, "windows")? {
        let id = window["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or("Invalid canvas window ID")?;
        // Presented windows ignore their saved normal canvas position. Their
        // actual screen bounds already include their output origin and chrome.
        let presented = window["output_coordinates"].as_bool().unwrap_or(false)
            || window["maximized"].as_bool().unwrap_or(false)
            || window["fullscreen"].as_bool().unwrap_or(false);
        let bounds = if presented {
            snapshot.screen_to_canvas(rect(
                number(window, "x")?,
                number(window, "y")?,
                number(window, "width")?,
                number(window, "height")?,
            )?)
        } else {
            // Status reports titlebar height in screen coordinates, unlike
            // content dimensions and the saved canvas position.
            let titlebar = number(window, "titlebar_height")?;
            if titlebar < 0.0 {
                return Err("Invalid canvas titlebar height".into());
            }
            let content = rect(
                number(window, "canvas_x")?,
                number(window, "canvas_y")?,
                number(window, "content_width")?,
                number(window, "content_height")?,
            )?;
            Rect::new(
                content.x0,
                content.y0,
                content.x1,
                content.y1 + titlebar / scale,
            )
        };
        validate_rect(bounds)?;
        snapshot.windows.push(MapWindow {
            id,
            title: window["title"].as_str().unwrap_or_default().to_string(),
            app_id: window["app_id"].as_str().unwrap_or_default().to_string(),
            bounds,
            focused: window["focused"].as_bool().unwrap_or(false),
            output_coordinates: presented,
        });
    }
    Ok(snapshot)
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], String> {
    value[field]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("Missing canvas {field}"))
}

fn number(value: &Value, field: &str) -> Result<f64, String> {
    value[field]
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("Invalid canvas {field}"))
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Result<Rect, String> {
    let rect = Rect::new(x, y, x + width, y + height);
    validate_rect(rect)?;
    Ok(rect)
}

fn validate_rect(rect: Rect) -> Result<(), String> {
    // The compositor uses i32 canvas coordinates. A generous upper bound also
    // prevents pathological JSON from overflowing later union/grid arithmetic.
    if [rect.x0, rect.y0, rect.x1, rect.y1]
        .into_iter()
        .all(|value| value.is_finite() && value.abs() <= 1e12)
        && rect.width() > 0.0
        && rect.height() > 0.0
    {
        Ok(())
    } else {
        Err("Invalid canvas bounds".into())
    }
}
