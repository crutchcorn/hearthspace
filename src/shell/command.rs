#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnTarget {
    A11yTest,
    Foot,
}

impl SpawnTarget {
    pub const ALL: [Self; 2] = [Self::A11yTest, Self::Foot];

    pub fn label(self) -> &'static str {
        match self {
            Self::A11yTest => "A11yTest",
            Self::Foot => "Foot",
        }
    }

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

#[derive(Debug, Clone, PartialEq)]
pub enum ShellCommand {
    Spawn(SpawnTarget),
    LaunchApp(String),
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    ZoomIn,
    ZoomOut,
    LogAccessibilityTree,
    KeyDown(u32),
    KeyUp(u32),
    PointerMotionAbs { x: f64, y: f64 },
    PointerMotionRel { dx: f64, dy: f64 },
    PointerButtonDown(u32),
    PointerButtonUp(u32),
    Axis { horizontal: f64, vertical: f64 },
    Screenshot,
    Quit,
}

impl ShellCommand {
    pub const STATIC_COMMANDS: [Self; 9] = [
        Self::Spawn(SpawnTarget::A11yTest),
        Self::Spawn(SpawnTarget::Foot),
        Self::PanLeft,
        Self::PanRight,
        Self::PanUp,
        Self::PanDown,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::LogAccessibilityTree,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Spawn(target) => target.label(),
            Self::LaunchApp(_) => "APP",
            Self::PanLeft => "LEFT",
            Self::PanRight => "RIGHT",
            Self::PanUp => "UP",
            Self::PanDown => "DOWN",
            Self::ZoomIn => "ZOOM+",
            Self::ZoomOut => "ZOOM-",
            Self::LogAccessibilityTree => "LOG",
            Self::KeyDown(_) => "KEYDOWN",
            Self::KeyUp(_) => "KEYUP",
            Self::PointerMotionAbs { .. } => "PTRABS",
            Self::PointerMotionRel { .. } => "PTRREL",
            Self::PointerButtonDown(_) => "BTNDOWN",
            Self::PointerButtonUp(_) => "BTNUP",
            Self::Axis { .. } => "AXIS",
            Self::Screenshot => "SHOT",
            Self::Quit => "QUIT",
        }
    }

    pub fn wire_name(&self) -> String {
        match self {
            Self::Spawn(SpawnTarget::A11yTest) => "spawn a11y-test".to_string(),
            Self::Spawn(SpawnTarget::Foot) => "spawn foot".to_string(),
            Self::LaunchApp(app_id) => format!("launch-app {app_id}"),
            Self::PanLeft => "pan-left".to_string(),
            Self::PanRight => "pan-right".to_string(),
            Self::PanUp => "pan-up".to_string(),
            Self::PanDown => "pan-down".to_string(),
            Self::ZoomIn => "zoom-in".to_string(),
            Self::ZoomOut => "zoom-out".to_string(),
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

        let mut parts = input.split_whitespace();
        let command = parts.next()?;

        Some(match command {
            "spawn" => Self::Spawn(SpawnTarget::parse(parts.next()?)?),
            "pan-left" => Self::PanLeft,
            "pan-right" => Self::PanRight,
            "pan-up" => Self::PanUp,
            "pan-down" => Self::PanDown,
            "zoom-in" => Self::ZoomIn,
            "zoom-out" => Self::ZoomOut,
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
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_commands_round_trip_wire_names() {
        for command in ShellCommand::STATIC_COMMANDS {
            assert_eq!(ShellCommand::parse(&command.wire_name()), Some(command));
        }
    }

    #[test]
    fn launch_app_parses_desktop_entry_id() {
        assert_eq!(
            ShellCommand::parse("launch-app org.gnome.Calculator.desktop"),
            Some(ShellCommand::LaunchApp(
                "org.gnome.Calculator.desktop".to_string()
            ))
        );
    }

    #[test]
    fn waydriver_input_commands_parse() {
        assert_eq!(
            ShellCommand::parse("key-down 30"),
            Some(ShellCommand::KeyDown(30))
        );
        assert_eq!(
            ShellCommand::parse("key-up 0xff"),
            Some(ShellCommand::KeyUp(255))
        );
        assert_eq!(
            ShellCommand::parse("pointer-motion-abs 10.5 20"),
            Some(ShellCommand::PointerMotionAbs { x: 10.5, y: 20.0 })
        );
        assert_eq!(
            ShellCommand::parse("pointer-motion-rel -3 4.25"),
            Some(ShellCommand::PointerMotionRel { dx: -3.0, dy: 4.25 })
        );
        assert_eq!(
            ShellCommand::parse("pointer-button-down 272"),
            Some(ShellCommand::PointerButtonDown(272))
        );
        assert_eq!(
            ShellCommand::parse("axis 0 -120"),
            Some(ShellCommand::Axis {
                horizontal: 0.0,
                vertical: -120.0,
            })
        );
        assert_eq!(
            ShellCommand::parse("screenshot"),
            Some(ShellCommand::Screenshot)
        );
        assert_eq!(ShellCommand::parse("quit"), Some(ShellCommand::Quit));
    }
}
