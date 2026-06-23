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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellCommand {
    Spawn(SpawnTarget),
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    ZoomIn,
    ZoomOut,
    LogAccessibilityTree,
    OcrFocusedWindow,
}

impl ShellCommand {
    pub const ALL: [Self; 10] = [
        Self::Spawn(SpawnTarget::A11yTest),
        Self::Spawn(SpawnTarget::Foot),
        Self::PanLeft,
        Self::PanRight,
        Self::PanUp,
        Self::PanDown,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::LogAccessibilityTree,
        Self::OcrFocusedWindow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Spawn(target) => target.label(),
            Self::PanLeft => "LEFT",
            Self::PanRight => "RIGHT",
            Self::PanUp => "UP",
            Self::PanDown => "DOWN",
            Self::ZoomIn => "ZOOM+",
            Self::ZoomOut => "ZOOM-",
            Self::LogAccessibilityTree => "LOG",
            Self::OcrFocusedWindow => "OCR",
        }
    }

    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Spawn(SpawnTarget::A11yTest) => "spawn a11y-test",
            Self::Spawn(SpawnTarget::Foot) => "spawn foot",
            Self::PanLeft => "pan-left",
            Self::PanRight => "pan-right",
            Self::PanUp => "pan-up",
            Self::PanDown => "pan-down",
            Self::ZoomIn => "zoom-in",
            Self::ZoomOut => "zoom-out",
            Self::LogAccessibilityTree => "log-a11y-tree",
            Self::OcrFocusedWindow => "ocr-focused-window",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
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
            "ocr-focused-window" => Self::OcrFocusedWindow,
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
