use hearthspace_common::frame_layout::{ClockDateFormat, LayoutTarget, ObjectPlacement};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub session: u64,
    pub output: String,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
pub enum Action {
    Done,
    Cancel,
    Confirm,
    AllowHideTools {
        value: bool,
    },
    Select {
        target: LayoutTarget,
    },
    Place {
        target: LayoutTarget,
        placement: ObjectPlacement,
    },
    Attach {
        target: LayoutTarget,
    },
    Remove {
        target: LayoutTarget,
    },
    Ungroup {
        target: LayoutTarget,
    },
    ToggleExpanded {
        target: LayoutTarget,
    },
    ClockDateFormat {
        target: LayoutTarget,
        value: ClockDateFormat,
    },
}

impl Action {
    pub fn target(&self) -> Option<&LayoutTarget> {
        match self {
            Self::Select { target }
            | Self::Place { target, .. }
            | Self::Attach { target }
            | Self::Remove { target }
            | Self::Ungroup { target }
            | Self::ToggleExpanded { target }
            | Self::ClockDateFormat { target, .. } => Some(target),
            Self::Done | Self::Cancel | Self::Confirm | Self::AllowHideTools { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_hide_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_tools: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsSnapshot {
    pub version: u32,
    pub allow_hide_tools: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockFormat {
    pub output: String,
    pub value: ClockDateFormat,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Focus {
    pub output: String,
    pub target: LayoutTarget,
    #[serde(default)]
    pub reverse: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub app_id: String,
    pub output: String,
    pub target: LayoutTarget,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FocusRegion {
    pub output: String,
    pub target: LayoutTarget,
    pub rect: super::Rect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Available {
    pub output: String,
    pub id: String,
    pub available: bool,
}
