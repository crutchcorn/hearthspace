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

#[derive(Debug, Clone, PartialEq, Eq)]
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
            _ => return None,
        })
    }
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
}
