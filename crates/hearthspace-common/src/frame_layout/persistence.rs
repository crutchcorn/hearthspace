//! The legacy disk format had implicit singleton controls. Migrate only at this boundary.
use super::{ClockDateFormat, FrameGroup, FrameLayout, LayoutTarget, ObjectPlacement, WIDGETS};
use crate::widgets::WidgetStore;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(default)]
pub(super) struct PersistedFrameLayout {
    widgets: Option<WidgetStore>,
    placements: BTreeMap<String, ObjectPlacement>,
    groups: BTreeMap<String, FrameGroup>,
    expanded: BTreeMap<String, bool>,
    allow_hide_tools: bool,
    clock_date_format: ClockDateFormat,
}

impl Default for PersistedFrameLayout {
    fn default() -> Self {
        // Missing fields in old files carried the starter system group.
        let members = super::SYSTEM_WIDGETS
            .iter()
            .map(|kind| LayoutTarget::widget(*kind))
            .collect();
        let group = FrameGroup {
            id: "system".into(),
            members,
        };
        Self {
            widgets: None,
            placements: [("group:system".into(), super::starter_placement("clock"))].into(),
            groups: [(group.id.clone(), group)].into(),
            expanded: BTreeMap::new(),
            allow_hide_tools: false,
            clock_date_format: ClockDateFormat::None,
        }
    }
}

impl TryFrom<PersistedFrameLayout> for FrameLayout {
    type Error = &'static str;

    fn try_from(saved: PersistedFrameLayout) -> Result<Self, Self::Error> {
        if let Some(widgets) = saved.widgets {
            if !saved.expanded.is_empty() {
                return Err("legacy preferences cannot override widget instances");
            }
            let layout = Self {
                widgets,
                placements: saved.placements,
                groups: saved.groups,
                allow_hide_tools: saved.allow_hide_tools,
                clock_date_format: saved.clock_date_format,
            };
            layout.validate()?;
            return Ok(layout);
        }
        saved.validate_legacy()?;
        let mut layout = Self::empty();
        layout.allow_hide_tools = saved.allow_hide_tools;
        layout.clock_date_format = saved.clock_date_format;
        let mut replacements = BTreeMap::new();
        for kind in WIDGETS.iter().copied().filter(|kind| *kind != "launcher") {
            let target = LayoutTarget::widget(kind);
            let grouped = saved
                .groups
                .values()
                .any(|group| group.members.contains(&target));
            let placement = saved
                .placements
                .get(&target.key())
                .copied()
                .unwrap_or_else(|| super::starter_placement(kind));
            if placement == ObjectPlacement::Hidden && !grouped {
                continue;
            }
            let id = layout
                .add_widget(kind, placement)
                .expect("bounded legacy starter widgets");
            layout.set_starter_availability(&id);
            if let Some(expanded) = saved.expanded.get(kind)
                && *expanded != layout.is_expanded(&id)
            {
                layout.toggle_expanded(&id);
            }
            replacements.insert(kind.to_owned(), id);
        }
        for (key, placement) in saved.placements {
            if key.starts_with("widget:") {
                continue;
            }
            layout.placements.insert(key, placement);
        }
        for (id, mut group) in saved.groups {
            let default = group
                .members
                .first()
                .map_or(ObjectPlacement::canvas(), |member| {
                    if member.kind == super::TargetKind::Widget {
                        super::starter_placement(&member.id)
                    } else {
                        super::default_placement(member)
                    }
                });
            layout
                .placements
                .entry(format!("group:{id}"))
                .or_insert(default);
            for member in &mut group.members {
                if member.kind == super::TargetKind::Widget
                    && let Some(replacement) = replacements.get(&member.id)
                {
                    member.id = replacement.clone();
                    layout.placements.remove(&member.key());
                }
            }
            layout.groups.insert(id, group);
        }
        layout.validate()?;
        Ok(layout)
    }
}

impl PersistedFrameLayout {
    fn validate_legacy(&self) -> Result<(), &'static str> {
        use super::TargetKind;
        let valid_widget = |id: &str| id != "launcher" && WIDGETS.contains(&id);
        if self.placements.len() > 8192
            || self.groups.len() > 1024
            || self.expanded.len() > WIDGETS.len()
        {
            return Err("too many legacy layout entries");
        }
        for (key, placement) in &self.placements {
            let target = if let Some(id) = key.strip_prefix("widget:") {
                if !valid_widget(id) {
                    return Err("unknown legacy widget");
                }
                LayoutTarget::widget(id)
            } else {
                LayoutTarget::from_key(key).ok_or("invalid legacy layout target")?
            };
            if !placement.valid()
                || (target.kind == TargetKind::Group && !self.groups.contains_key(&target.id))
                || (*placement == ObjectPlacement::Hidden && target.kind != TargetKind::Widget)
                || (*placement == ObjectPlacement::Hidden
                    && target.id == "tools"
                    && !self.allow_hide_tools)
            {
                return Err("invalid legacy placement");
            }
        }
        let mut claimed = std::collections::BTreeSet::new();
        for (id, group) in &self.groups {
            if id != &group.id
                || !LayoutTarget::new(TargetKind::Group, id).valid()
                || group.members.len() < 2
                || group.members.len() > 256
            {
                return Err("invalid legacy group");
            }
            for member in &group.members {
                let valid = if member.kind == TargetKind::Widget {
                    valid_widget(&member.id)
                } else {
                    member.valid()
                };
                if !valid
                    || member.kind == TargetKind::Group
                    || !claimed.insert(member.key())
                    || self.placements.contains_key(&member.key())
                {
                    return Err("invalid legacy group member");
                }
            }
        }
        if self
            .expanded
            .keys()
            .any(|kind| !super::EXPANDABLE_WIDGETS.contains(&kind.as_str()))
        {
            return Err("unknown legacy widget preference");
        }
        Ok(())
    }
}
