pub const WIDGETS: &[&str] = &[
    "notifications",
    "clock",
    "wifi",
    "volume",
    "brightness",
    "battery",
    "tray",
    "power",
    "tools",
    "apps",
    "launcher",
];
pub const SYSTEM_WIDGETS: &[&str] = &[
    "clock",
    "wifi",
    "volume",
    "brightness",
    "battery",
    "tray",
    "power",
];
pub const EXPANDABLE_WIDGETS: &[&str] = &[
    "notifications",
    "clock",
    "wifi",
    "volume",
    "brightness",
    "battery",
];

pub fn widget_title(id: &str) -> &'static str {
    match id {
        "notifications" => "Notifications",
        "clock" => "Clock",
        "wifi" => "Wi-Fi",
        "volume" => "Sound",
        "brightness" => "Brightness",
        "battery" => "Battery",
        "tray" => "System tray",
        "power" => "Power",
        "tools" => "Canvas tools",
        "apps" => "App dock",
        "launcher" => "App launcher",
        _ => "Item",
    }
}

pub fn starter_placement(kind: &str) -> super::ObjectPlacement {
    use super::{FrameEdge, ObjectPlacement};
    match kind {
        "notifications" => ObjectPlacement::frame(FrameEdge::Top, 0.0),
        "tools" => ObjectPlacement::frame(FrameEdge::Bottom, 0.0),
        "apps" | "launcher" => ObjectPlacement::frame(FrameEdge::Bottom, 1.0),
        _ => ObjectPlacement::frame(FrameEdge::Top, 1.0),
    }
}
