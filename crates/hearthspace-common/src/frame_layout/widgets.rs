//! Widget lifetime and mixed-object placement change together under one writer.
use super::{ClockDateFormat, FrameLayout, LayoutTarget, ObjectPlacement, TargetKind};
use crate::widgets::{InstanceId, Preferences, WidgetKind, WidgetStore};
use std::collections::BTreeMap;

impl FrameLayout {
    pub fn empty() -> Self {
        Self {
            widgets: WidgetStore::new(),
            placements: BTreeMap::new(),
            groups: BTreeMap::new(),
            allow_hide_tools: false,
            clock_date_format: ClockDateFormat::None,
        }
    }

    pub fn widget_kind(&self, id: &str) -> Option<&'static str> {
        let id = id.parse::<InstanceId>().ok()?;
        self.widgets.get(id).map(|instance| instance.kind().key())
    }

    /// Find an existing copy for a global shortcut; a widget action retains its own ID.
    pub fn widget_id(&self, kind: &str) -> Option<String> {
        self.widgets
            .instances()
            .find(|instance| instance.kind().key() == kind)
            .map(|instance| instance.id().to_string())
    }

    pub fn add_widget(
        &mut self,
        kind: &str,
        placement: ObjectPlacement,
    ) -> Result<String, &'static str> {
        let kind = WidgetKind::from_key(kind).ok_or("unknown widget kind")?;
        if !placement.valid() || placement == ObjectPlacement::Hidden {
            return Err("invalid widget placement");
        }
        if self.placements.len() >= 8192 {
            return Err("too many layout placements");
        }
        let preferences = Preferences {
            hide_when_unavailable: false,
            expanded_details: matches!(kind, WidgetKind::Clock | WidgetKind::Notifications),
            clock_date_format: None,
        };
        let id = self
            .widgets
            .add(kind, preferences)
            .map_err(|_| "could not add widget")?;
        let id = id.to_string();
        self.placements
            .insert(LayoutTarget::widget(&id).key(), placement);
        Ok(id)
    }

    pub(super) fn set_starter_availability(&mut self, id: &str) {
        let identity = id.parse::<InstanceId>().expect("registered starter");
        let instance = self.widgets.get(identity).expect("registered starter");
        let mut preferences = instance.preferences().clone();
        preferences.hide_when_unavailable = matches!(
            instance.kind(),
            WidgetKind::Battery | WidgetKind::ApplicationTray
        );
        self.widgets
            .set_preferences(identity, preferences)
            .expect("valid starter preferences");
    }

    pub fn widget_hides_when_unavailable(&self, id: &str) -> bool {
        id.parse::<InstanceId>()
            .ok()
            .and_then(|id| self.widgets.get(id))
            .is_some_and(|instance| instance.preferences().hide_when_unavailable)
    }

    pub fn remove_widget(&mut self, id: &str) -> Result<(), &'static str> {
        let identity = id
            .parse::<InstanceId>()
            .map_err(|_| "invalid widget identity")?;
        if self.widgets.get(identity).is_none() {
            return Err("widget disappeared");
        }
        let target = LayoutTarget::widget(id);
        if !self.placements.contains_key(&target.key()) && self.group_for(&target).is_none() {
            return Err("widget has no placement");
        }
        self.widgets
            .remove(identity)
            .map_err(|_| "widget disappeared")?;
        self.remove_target(&target);
        Ok(())
    }

    pub fn is_expanded(&self, id: &str) -> bool {
        let Ok(id) = id.parse::<InstanceId>() else {
            return false;
        };
        self.widgets
            .get(id)
            .is_some_and(|instance| instance.preferences().expanded_details)
    }

    pub fn toggle_expanded(&mut self, id: &str) {
        if !self
            .widget_kind(id)
            .is_some_and(|kind| super::EXPANDABLE_WIDGETS.contains(&kind))
        {
            return;
        }
        let id = id.parse::<InstanceId>().expect("registered identity");
        let mut preferences = self
            .widgets
            .get(id)
            .expect("registered widget")
            .preferences()
            .clone();
        preferences.expanded_details = !preferences.expanded_details;
        self.widgets
            .set_preferences(id, preferences)
            .expect("valid preferences");
    }

    pub fn widget_clock_date_format(&self, id: &str) -> ClockDateFormat {
        let Some(instance) = id
            .parse::<InstanceId>()
            .ok()
            .and_then(|id| self.widgets.get(id))
        else {
            return self.clock_date_format;
        };
        instance
            .preferences()
            .clock_date_format
            .as_ref()
            .and_then(|format| {
                serde_json::from_value(serde_json::Value::String(format.clone())).ok()
            })
            .unwrap_or(self.clock_date_format)
    }

    pub fn set_widget_clock_date_format(
        &mut self,
        id: &str,
        value: ClockDateFormat,
    ) -> Result<(), &'static str> {
        let id = id
            .parse::<InstanceId>()
            .map_err(|_| "invalid widget identity")?;
        let instance = self.widgets.get(id).ok_or("widget disappeared")?;
        if instance.kind() != WidgetKind::Clock {
            return Err("widget is not a clock");
        }
        let mut preferences = instance.preferences().clone();
        let value = serde_json::to_value(value).expect("serializable date format");
        preferences.clock_date_format = Some(value.as_str().expect("date format name").into());
        self.widgets
            .set_preferences(id, preferences)
            .map_err(|_| "invalid widget preferences")
    }

    pub(super) fn validate_widget_preferences(&self, id: &str) -> Result<(), &'static str> {
        let id = id
            .parse::<InstanceId>()
            .map_err(|_| "invalid widget identity")?;
        let instance = self.widgets.get(id).ok_or("widget disappeared")?;
        if let Some(format) = &instance.preferences().clock_date_format {
            let value = serde_json::Value::String(format.clone());
            serde_json::from_value::<ClockDateFormat>(value)
                .map_err(|_| "invalid clock date format")?;
        }
        Ok(())
    }

    pub fn widget_targets(&self) -> impl Iterator<Item = LayoutTarget> + '_ {
        self.widgets
            .instances()
            .map(|instance| LayoutTarget::new(TargetKind::Widget, instance.id().to_string()))
    }
}
