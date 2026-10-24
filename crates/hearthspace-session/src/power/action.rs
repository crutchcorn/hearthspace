#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Suspend,
    Reboot,
    PowerOff,
}

impl Action {
    pub fn parse(command: &str) -> Option<Self> {
        match command {
            "suspend" => Some(Self::Suspend),
            "reboot" => Some(Self::Reboot),
            "power_off" => Some(Self::PowerOff),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Suspend => "Suspend",
            Self::Reboot => "Reboot",
            Self::PowerOff => "Turn off",
        }
    }
}
