use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub test: bool,
    pub background_mode: BackgroundMode,
    pub background_image_path: Option<String>,
    pub monitors: Vec<MonitorConfig>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BackgroundMode {
    #[default]
    RepeatWithCanvas,
    Stretch,
    Contains,
    Cover,
}

impl BackgroundMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RepeatWithCanvas => "repeat-with-canvas",
            Self::Stretch => "stretch",
            Self::Contains => "contains",
            Self::Cover => "cover",
        }
    }

    pub fn from_legacy_moves_with_canvas(value: bool) -> Self {
        if value {
            Self::RepeatWithCanvas
        } else {
            Self::Cover
        }
    }
}

impl std::fmt::Display for BackgroundMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BackgroundMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "repeat-with-canvas" => Ok(Self::RepeatWithCanvas),
            "stretch" => Ok(Self::Stretch),
            "contains" => Ok(Self::Contains),
            "cover" => Ok(Self::Cover),
            _ => Err(format!("invalid background mode: {value:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorConfig {
    pub id: String,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub scale: i32,
    pub x: i32,
    pub y: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            test: false,
            background_mode: BackgroundMode::default(),
            background_image_path: None,
            monitors: vec![
                MonitorConfig {
                    id: "primary".to_string(),
                    name: "Primary".to_string(),
                    width: 1920,
                    height: 1080,
                    scale: 1,
                    x: 0,
                    y: 0,
                },
                MonitorConfig {
                    id: "secondary".to_string(),
                    name: "Secondary".to_string(),
                    width: 1920,
                    height: 1080,
                    scale: 1,
                    x: 1920,
                    y: 0,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsRequest {
    Get,
    SetTest(bool),
    SetBackgroundMode(BackgroundMode),
    SetBackgroundImagePath(Option<String>),
    SetMonitorPosition { id: String, x: i32, y: i32 },
    SetMonitorScale { id: String, scale: i32 },
}
