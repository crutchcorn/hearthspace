use super::{SYSTEM_WIDGETS, WIDGETS};
use crate::widgets::WidgetStore;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Widget,
    Item,
    Window,
    Group,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutTarget {
    pub kind: TargetKind,
    pub id: String,
}

impl LayoutTarget {
    pub fn new(kind: TargetKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    pub fn widget(id: impl Into<String>) -> Self {
        Self::new(TargetKind::Widget, id)
    }

    pub fn key(&self) -> String {
        let kind = match self.kind {
            TargetKind::Widget => "widget",
            TargetKind::Item => "item",
            TargetKind::Window => "window",
            TargetKind::Group => "group",
        };
        format!("{kind}:{}", self.id)
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let (kind, id) = key.split_once(':')?;
        let kind = match kind {
            "widget" => TargetKind::Widget,
            "item" => TargetKind::Item,
            "window" => TargetKind::Window,
            "group" => TargetKind::Group,
            _ => return None,
        };
        let target = Self::new(kind, id);
        target.valid().then_some(target)
    }

    pub fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 1024
            && !self.id.chars().any(char::is_control)
            && (self.kind != TargetKind::Widget
                || self.id.parse::<crate::widgets::InstanceId>().is_ok())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameEdge {
    Top,
    Right,
    Bottom,
    Left,
}

impl FrameEdge {
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "surface", rename_all = "lowercase")]
pub enum ObjectPlacement {
    Canvas {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
    },
    Background {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        y: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
    },
    Frame {
        edge: FrameEdge,
        position: f64,
    },
    Hidden,
}

/// Optional dimensions let a control retain its measured size across canvas moves.
pub type CanvasBounds = (Option<f64>, Option<f64>, Option<f64>, Option<f64>);

impl ObjectPlacement {
    pub fn canvas() -> Self {
        Self::Canvas {
            x: None,
            y: None,
            width: None,
            height: None,
        }
    }
    pub fn background() -> Self {
        Self::Background {
            x: None,
            y: None,
            width: None,
            height: None,
        }
    }
    pub fn frame(edge: FrameEdge, position: f64) -> Self {
        Self::Frame {
            edge,
            position: position.clamp(0.0, 1.0),
        }
    }
    pub fn is_frame(self) -> bool {
        matches!(self, Self::Frame { .. })
    }
    pub fn edge(self) -> Option<FrameEdge> {
        if let Self::Frame { edge, .. } = self {
            Some(edge)
        } else {
            None
        }
    }

    /// The optional saved canvas dimensions. Windows retain their client-owned dimensions.
    pub fn canvas_bounds(self) -> Option<CanvasBounds> {
        match self {
            Self::Canvas {
                x,
                y,
                width,
                height,
            }
            | Self::Background {
                x,
                y,
                width,
                height,
            } => Some((x, y, width, height)),
            _ => None,
        }
    }

    pub fn valid(self) -> bool {
        match self {
            Self::Frame { position, .. } => position.is_finite() && (0.0..=1.0).contains(&position),
            Self::Canvas {
                x,
                y,
                width,
                height,
            }
            | Self::Background {
                x,
                y,
                width,
                height,
            } => {
                [x, y]
                    .into_iter()
                    .flatten()
                    .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                    && [width, height]
                        .into_iter()
                        .flatten()
                        .all(|v| v.is_finite() && v > 0.0 && v <= 100_000.0)
            }
            Self::Hidden => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameGroup {
    pub id: String,
    pub members: Vec<LayoutTarget>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClockDateFormat {
    #[default]
    None,
    Weekday,
    Short,
    Numeric,
    Long,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "super::persistence::PersistedFrameLayout")]
pub struct FrameLayout {
    pub widgets: WidgetStore,
    pub placements: BTreeMap<String, ObjectPlacement>,
    pub groups: BTreeMap<String, FrameGroup>,
    pub allow_hide_tools: bool,
    pub clock_date_format: ClockDateFormat,
}

pub fn default_placement(target: &LayoutTarget) -> ObjectPlacement {
    match target.kind {
        TargetKind::Widget => ObjectPlacement::canvas(),
        TargetKind::Item => ObjectPlacement::background(),
        TargetKind::Window | TargetKind::Group => ObjectPlacement::canvas(),
    }
}

impl Default for FrameLayout {
    fn default() -> Self {
        let mut layout = Self::empty();
        for kind in WIDGETS.iter().copied().filter(|kind| *kind != "launcher") {
            let placement = super::starter_placement(kind);
            let id = layout.add_widget(kind, placement).expect("starter widget");
            layout.set_starter_availability(&id);
        }
        let group = FrameGroup {
            id: "system".into(),
            members: SYSTEM_WIDGETS
                .iter()
                .map(|kind| LayoutTarget::widget(layout.widget_id(kind).expect("starter member")))
                .collect(),
        };
        for member in &group.members {
            layout.placements.remove(&member.key());
        }
        layout.placements.insert(
            "group:system".into(),
            ObjectPlacement::frame(FrameEdge::Top, 1.0),
        );
        layout.groups.insert(group.id.clone(), group);
        layout
    }
}

impl FrameLayout {
    pub fn group_for(&self, target: &LayoutTarget) -> Option<&FrameGroup> {
        if target.kind == TargetKind::Group {
            self.groups.get(&target.id)
        } else {
            self.groups
                .values()
                .find(|group| group.members.contains(target))
        }
    }

    pub fn placement(&self, target: &LayoutTarget) -> ObjectPlacement {
        let group = self.group_for(target);
        let key = group
            .map(|group| format!("group:{}", group.id))
            .unwrap_or_else(|| target.key());
        self.placements.get(&key).copied().unwrap_or_else(|| {
            default_placement(group.and_then(|g| g.members.first()).unwrap_or(target))
        })
    }

    pub fn set_allow_hide_tools(&mut self, allow: bool) {
        self.allow_hide_tools = allow;
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.placements.len() > 8192 || self.groups.len() > 1024 {
            return Err("too many frame layout entries");
        }
        for (key, placement) in &self.placements {
            let target = LayoutTarget::from_key(key).ok_or("invalid layout target")?;
            if !placement.valid()
                || (target.kind == TargetKind::Group && !self.groups.contains_key(&target.id))
                || (*placement == ObjectPlacement::Hidden)
                || (target.kind == TargetKind::Widget && self.widget_kind(&target.id).is_none())
            {
                return Err("invalid layout placement");
            }
        }
        let mut claimed = std::collections::BTreeSet::new();
        for (id, group) in &self.groups {
            if id != &group.id
                || !LayoutTarget::new(TargetKind::Group, id).valid()
                || group.members.len() < 2
                || group.members.len() > 256
            {
                return Err("invalid layout group");
            }
            for member in &group.members {
                if !member.valid()
                    || (member.kind == TargetKind::Widget && self.widget_kind(&member.id).is_none())
                    || member.kind == TargetKind::Group
                    || !claimed.insert(member.key())
                    || self.placements.contains_key(&member.key())
                {
                    return Err("invalid or duplicate layout group member");
                }
            }
        }
        for instance in self.widgets.instances() {
            let id = instance.id().to_string();
            let target = LayoutTarget::widget(&id);
            if !self.placements.contains_key(&target.key()) && self.group_for(&target).is_none() {
                return Err("widget has no placement");
            }
            self.validate_widget_preferences(&id)?;
        }
        Ok(())
    }
}
