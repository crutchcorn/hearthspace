use super::WidgetKind;

impl WidgetKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Notifications => "notifications",
            Self::Clock => "clock",
            Self::Wifi => "wifi",
            Self::Sound => "volume",
            Self::Brightness => "brightness",
            Self::Battery => "battery",
            Self::ApplicationTray => "tray",
            Self::Power => "power",
            Self::CanvasTools => "tools",
            Self::AppDock => "apps",
            Self::AppLauncher => "launcher",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "notifications" => Self::Notifications,
            "clock" => Self::Clock,
            "wifi" => Self::Wifi,
            "volume" => Self::Sound,
            "brightness" => Self::Brightness,
            "battery" => Self::Battery,
            "tray" => Self::ApplicationTray,
            "power" => Self::Power,
            "tools" => Self::CanvasTools,
            "apps" => Self::AppDock,
            "launcher" => Self::AppLauncher,
            _ => return None,
        })
    }
}
