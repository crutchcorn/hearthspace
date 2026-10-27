//! Shared compositor control commands. Presentation labels belong to clients.
mod canvas_tool;
pub use canvas_tool::CanvasTool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnTarget {
    A11yTest,
    Foot,
}

impl SpawnTarget {
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::A11yTest => "a11y-test",
            Self::Foot => "foot",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        Some(match input {
            "a11y-test" => Self::A11yTest,
            "foot" => Self::Foot,
            _ => return None,
        })
    }
}

/// Relative steps are resolved by the compositor; `To` is an absolute canvas scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoomOperation {
    In,
    Out,
    To(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShellCommand {
    CanvasTool(CanvasTool),
    GroupSelectionToggle,
    GroupClear,
    GroupGather,
    GroupStatus,
    LockSession,
    ListApps,
    ListWindows,
    FocusWindow(u64),
    CloseWindow(u64),
    MoveWindow {
        id: u64,
        x: i32,
        y: i32,
    },
    ResizeWindow {
        id: u64,
        width: u32,
        height: u32,
    },
    FocusNextWindow,
    FocusPreviousWindow,
    FocusLauncher,
    AcceptShellEvent(u64),
    FullscreenWindow {
        id: u64,
        enabled: bool,
    },
    MaximizeWindow {
        id: u64,
        enabled: bool,
    },
    RestoreWindow(u64),
    Spawn(SpawnTarget),
    LaunchApp(String),
    LaunchAppOnOutput {
        app_id: String,
        output_name: String,
    },
    OpenSearchSettings {
        output_name: Option<String>,
    },
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    PanTo {
        x: i32,
        y: i32,
    },
    ZoomIn,
    ZoomOut,
    ZoomTo(f64),
    ZoomOnOutput {
        output_name: String,
        operation: ZoomOperation,
    },
    ReloadSettings,
    LogAccessibilityTree,
    KeyDown(u32),
    KeyUp(u32),
    PointerMotionAbs {
        x: f64,
        y: f64,
    },
    PointerMotionRel {
        dx: f64,
        dy: f64,
    },
    PointerButtonDown(u32),
    PointerButtonUp(u32),
    Axis {
        horizontal: f64,
        vertical: f64,
    },
    Screenshot,
    Quit,
}

impl ShellCommand {
    pub fn wire_name(&self) -> String {
        match self {
            Self::CanvasTool(tool) => format!("canvas-tool {}", tool.wire_name()),
            Self::GroupSelectionToggle => "group-selection-toggle".into(),
            Self::GroupClear => "group-clear".into(),
            Self::GroupGather => "group-gather".into(),
            Self::GroupStatus => "group-status".into(),
            Self::LockSession => "lock-session".into(),
            Self::ListApps => "apps".into(),
            Self::ListWindows => "windows".into(),
            Self::FocusWindow(id) => format!("focus-window {id}"),
            Self::CloseWindow(id) => format!("close-window {id}"),
            Self::MoveWindow { id, x, y } => format!("move-window {id} {x} {y}"),
            Self::ResizeWindow { id, width, height } => {
                format!("resize-window {id} {width} {height}")
            }
            Self::FocusNextWindow => "focus-next-window".into(),
            Self::FocusPreviousWindow => "focus-previous-window".into(),
            Self::FocusLauncher => "focus-launcher".into(),
            Self::AcceptShellEvent(token) => format!("shell-event-accept {token}"),
            Self::FullscreenWindow { id, enabled } => format!("fullscreen-window {id} {enabled}"),
            Self::MaximizeWindow { id, enabled } => format!("maximize-window {id} {enabled}"),
            Self::RestoreWindow(id) => format!("restore-window {id}"),
            Self::Spawn(SpawnTarget::A11yTest) => "spawn a11y-test".to_string(),
            Self::Spawn(SpawnTarget::Foot) => "spawn foot".to_string(),
            Self::LaunchApp(app_id) => format!("launch-app {app_id}"),
            Self::LaunchAppOnOutput {
                app_id,
                output_name,
            } => format!("launch-app-on-output {} {app_id}", hex_encode(output_name)),
            Self::OpenSearchSettings { output_name } => match output_name {
                Some(name) => format!("open-search-settings {}", hex_encode(name)),
                None => "open-search-settings".into(),
            },
            Self::PanLeft => "pan-left".to_string(),
            Self::PanRight => "pan-right".to_string(),
            Self::PanUp => "pan-up".to_string(),
            Self::PanDown => "pan-down".to_string(),
            Self::PanTo { x, y } => format!("pan-to {x} {y}"),
            Self::ZoomIn => "zoom-in".to_string(),
            Self::ZoomOut => "zoom-out".to_string(),
            Self::ZoomTo(scale) => format!("zoom-to {scale}"),
            Self::ZoomOnOutput {
                output_name,
                operation,
            } => {
                let operation = match operation {
                    ZoomOperation::In => "in".into(),
                    ZoomOperation::Out => "out".into(),
                    ZoomOperation::To(scale) => format!("to {scale}"),
                };
                format!("zoom-on-output {} {operation}", hex_encode(output_name))
            }
            Self::ReloadSettings => "reload-settings".to_string(),
            Self::LogAccessibilityTree => "log-a11y-tree".to_string(),
            Self::KeyDown(keycode) => format!("key-down {keycode}"),
            Self::KeyUp(keycode) => format!("key-up {keycode}"),
            Self::PointerMotionAbs { x, y } => format!("pointer-motion-abs {x} {y}"),
            Self::PointerMotionRel { dx, dy } => format!("pointer-motion-rel {dx} {dy}"),
            Self::PointerButtonDown(button) => format!("pointer-button-down {button}"),
            Self::PointerButtonUp(button) => format!("pointer-button-up {button}"),
            Self::Axis {
                horizontal,
                vertical,
            } => format!("axis {horizontal} {vertical}"),
            Self::Screenshot => "screenshot".to_string(),
            Self::Quit => "quit".to_string(),
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        if let Some(app_id) = input.strip_prefix("launch-app ") {
            let app_id = app_id.trim();
            return (!app_id.is_empty()).then(|| Self::LaunchApp(app_id.to_string()));
        }

        if let Some(rest) = input.strip_prefix("launch-app-on-output ") {
            let (output_name_hex, app_id) = rest.split_once(' ')?;
            let app_id = app_id.trim();
            let output_name = hex_decode_to_string(output_name_hex)?;
            return (!app_id.is_empty() && !output_name.is_empty()).then(|| {
                Self::LaunchAppOnOutput {
                    app_id: app_id.to_string(),
                    output_name,
                }
            });
        }

        let mut parts = input.split_whitespace();
        let command = parts.next()?;

        let parsed = match command {
            "canvas-tool" => Self::CanvasTool(CanvasTool::parse(parts.next()?)?),
            "open-search-settings" => Self::OpenSearchSettings {
                output_name: match parts.next() {
                    Some(encoded) => {
                        let name = hex_decode_to_string(encoded)?;
                        if name.is_empty() {
                            return None;
                        }
                        Some(name)
                    }
                    None => None,
                },
            },
            "group-selection-toggle" => Self::GroupSelectionToggle,
            "group-clear" => Self::GroupClear,
            "group-gather" => Self::GroupGather,
            "group-status" => Self::GroupStatus,
            "lock-session" => Self::LockSession,
            "apps" => Self::ListApps,
            "windows" => Self::ListWindows,
            "focus-window" => Self::FocusWindow(parts.next()?.parse().ok()?),
            "close-window" => Self::CloseWindow(parts.next()?.parse().ok()?),
            "move-window" => Self::MoveWindow {
                id: parts.next()?.parse().ok()?,
                x: parts.next()?.parse().ok()?,
                y: parts.next()?.parse().ok()?,
            },
            "resize-window" => Self::ResizeWindow {
                id: parts.next()?.parse().ok()?,
                width: parts.next()?.parse().ok()?,
                height: parts.next()?.parse().ok()?,
            },
            "focus-next-window" => Self::FocusNextWindow,
            "focus-previous-window" => Self::FocusPreviousWindow,
            "focus-launcher" => Self::FocusLauncher,
            "shell-event-accept" => Self::AcceptShellEvent(parts.next()?.parse().ok()?),
            "fullscreen-window" => Self::FullscreenWindow {
                id: parts.next()?.parse().ok()?,
                enabled: parts.next()?.parse().ok()?,
            },
            "maximize-window" => Self::MaximizeWindow {
                id: parts.next()?.parse().ok()?,
                enabled: parts.next()?.parse().ok()?,
            },
            "restore-window" => Self::RestoreWindow(parts.next()?.parse().ok()?),
            "spawn" => Self::Spawn(SpawnTarget::parse(parts.next()?)?),
            "pan-left" => Self::PanLeft,
            "pan-right" => Self::PanRight,
            "pan-up" => Self::PanUp,
            "pan-down" => Self::PanDown,
            "pan-to" => Self::PanTo {
                x: parts.next()?.parse().ok()?,
                y: parts.next()?.parse().ok()?,
            },
            "zoom-in" => Self::ZoomIn,
            "zoom-out" => Self::ZoomOut,
            "zoom-to" => Self::ZoomTo(parts.next()?.parse().ok()?),
            "zoom-on-output" => {
                let output_name = hex_decode_to_string(parts.next()?)?;
                if output_name.is_empty() {
                    return None;
                }
                let operation = match parts.next()? {
                    "in" => ZoomOperation::In,
                    "out" => ZoomOperation::Out,
                    "to" => ZoomOperation::To(parse_f64(parts.next()?)?),
                    _ => return None,
                };
                Self::ZoomOnOutput {
                    output_name,
                    operation,
                }
            }
            "reload-settings" => Self::ReloadSettings,
            "log-a11y-tree" => Self::LogAccessibilityTree,
            "key-down" => Self::KeyDown(parse_u32(parts.next()?)?),
            "key-up" => Self::KeyUp(parse_u32(parts.next()?)?),
            "pointer-motion-abs" => Self::PointerMotionAbs {
                x: parse_f64(parts.next()?)?,
                y: parse_f64(parts.next()?)?,
            },
            "pointer-motion-rel" => Self::PointerMotionRel {
                dx: parse_f64(parts.next()?)?,
                dy: parse_f64(parts.next()?)?,
            },
            "pointer-button-down" => Self::PointerButtonDown(parse_u32(parts.next()?)?),
            "pointer-button-up" => Self::PointerButtonUp(parse_u32(parts.next()?)?),
            "axis" => Self::Axis {
                horizontal: parse_f64(parts.next()?)?,
                vertical: parse_f64(parts.next()?)?,
            },
            "screenshot" => Self::Screenshot,
            "quit" => Self::Quit,
            _ => return None,
        };
        parts.next().is_none().then_some(parsed)
    }
}

fn parse_u32(input: &str) -> Option<u32> {
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        input.parse().ok()
    }
}

fn parse_f64(input: &str) -> Option<f64> {
    let value = input.parse().ok()?;
    f64::is_finite(value).then_some(value)
}

fn hex_encode(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(input.len() * 2);
    for byte in input.as_bytes() {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_decode_to_string(input: &str) -> Option<String> {
    if !input.len().is_multiple_of(2) {
        return None;
    }

    let bytes = input
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Some(hex_value(pair[0])? << 4 | hex_value(pair[1])?))
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
