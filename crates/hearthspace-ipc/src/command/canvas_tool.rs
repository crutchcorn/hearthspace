//! Explicit canvas selections shared by native controls and compositor input.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasTool {
    Select,
    Pan,
    Search,
    Group,
    Layout,
}

impl CanvasTool {
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Pan => "pan",
            Self::Search => "search",
            Self::Group => "group",
            Self::Layout => "layout",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        Some(match input {
            "select" => Self::Select,
            "pan" => Self::Pan,
            "search" => Self::Search,
            "group" => Self::Group,
            "layout" => Self::Layout,
            _ => return None,
        })
    }
}
