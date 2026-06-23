#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellCommand {
    SpawnApp,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    ZoomIn,
    ZoomOut,
}

impl ShellCommand {
    pub const ALL: [Self; 7] = [
        Self::SpawnApp,
        Self::PanLeft,
        Self::PanRight,
        Self::PanUp,
        Self::PanDown,
        Self::ZoomIn,
        Self::ZoomOut,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::SpawnApp => "SPAWN",
            Self::PanLeft => "LEFT",
            Self::PanRight => "RIGHT",
            Self::PanUp => "UP",
            Self::PanDown => "DOWN",
            Self::ZoomIn => "ZOOM+",
            Self::ZoomOut => "ZOOM-",
        }
    }

    pub fn wire_name(self) -> &'static str {
        match self {
            Self::SpawnApp => "spawn",
            Self::PanLeft => "pan-left",
            Self::PanRight => "pan-right",
            Self::PanUp => "pan-up",
            Self::PanDown => "pan-down",
            Self::ZoomIn => "zoom-in",
            Self::ZoomOut => "zoom-out",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        Some(match input.trim() {
            "spawn" => Self::SpawnApp,
            "pan-left" => Self::PanLeft,
            "pan-right" => Self::PanRight,
            "pan-up" => Self::PanUp,
            "pan-down" => Self::PanDown,
            "zoom-in" => Self::ZoomIn,
            "zoom-out" => Self::ZoomOut,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_commands_round_trip_wire_names() {
        for command in ShellCommand::ALL {
            assert_eq!(ShellCommand::parse(command.wire_name()), Some(command));
        }
    }
}
