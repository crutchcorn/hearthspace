//! The arrangement below is only a boundary fixture. It represents a caller's
//! existing owner; the lifecycle library contains no layout or geometry code.
use hearthspace_common::widgets::{Error, InstanceId, Preferences, WidgetKind, WidgetStore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
enum DesktopObject {
    Widget(InstanceId),
    FileIcon(u64),
    ApplicationWindow(u64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Placement {
    Canvas { output: String, x: f64, y: f64 },
    Frame { output: String, position: f64 },
}

impl Placement {
    fn is_valid(&self) -> bool {
        match self {
            Self::Canvas { x, y, .. } => x.is_finite() && y.is_finite(),
            Self::Frame { position, .. } => position.is_finite() && (0.0..=1.0).contains(position),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ArrangementError {
    Lifecycle(Error),
    InvalidGeometry,
    MissingPlacement,
}

/// One external arrangement ledger covers every object type. The composite
/// caller validates both owners before mutating either under exclusive access.
struct Desktop {
    widgets: WidgetStore,
    arrangement: BTreeMap<DesktopObject, Placement>,
}

impl Desktop {
    fn new() -> Self {
        Self {
            widgets: WidgetStore::new(),
            arrangement: BTreeMap::new(),
        }
    }

    fn add_widget(
        &mut self,
        kind: WidgetKind,
        placement: Placement,
    ) -> Result<InstanceId, ArrangementError> {
        if !placement.is_valid() {
            return Err(ArrangementError::InvalidGeometry);
        }
        let id = self
            .widgets
            .add(kind, Preferences::default())
            .map_err(ArrangementError::Lifecycle)?;
        self.arrangement
            .insert(DesktopObject::Widget(id), placement);
        Ok(id)
    }

    fn move_widget(
        &mut self,
        id: InstanceId,
        placement: Placement,
    ) -> Result<(), ArrangementError> {
        if self.widgets.get(id).is_none() {
            return Err(ArrangementError::Lifecycle(Error::UnknownInstance(id)));
        }
        if !placement.is_valid() {
            return Err(ArrangementError::InvalidGeometry);
        }
        let old = self
            .arrangement
            .get_mut(&DesktopObject::Widget(id))
            .ok_or(ArrangementError::MissingPlacement)?;
        *old = placement;
        Ok(())
    }

    fn remove_widget(&mut self, id: InstanceId) -> Result<(), ArrangementError> {
        if self.widgets.get(id).is_none() {
            return Err(ArrangementError::Lifecycle(Error::UnknownInstance(id)));
        }
        if !self.arrangement.contains_key(&DesktopObject::Widget(id)) {
            return Err(ArrangementError::MissingPlacement);
        }
        self.widgets
            .remove(id)
            .map_err(ArrangementError::Lifecycle)?;
        self.arrangement.remove(&DesktopObject::Widget(id));
        Ok(())
    }

    fn snapshot(&self) -> String {
        let lifecycle = self.widgets.to_json().unwrap();
        let arrangement = self.arrangement.iter().collect();
        let snapshot = DesktopSnapshot {
            lifecycle,
            arrangement,
        };
        serde_json::to_string(&snapshot).unwrap()
    }

    fn restart(snapshot: &str) -> Self {
        let snapshot: OwnedDesktopSnapshot = serde_json::from_str(snapshot).unwrap();
        Self {
            widgets: WidgetStore::from_json(&snapshot.lifecycle).unwrap(),
            arrangement: snapshot.arrangement.into_iter().collect(),
        }
    }
}

#[derive(Serialize)]
struct DesktopSnapshot<'a> {
    lifecycle: String,
    arrangement: Vec<(&'a DesktopObject, &'a Placement)>,
}

#[derive(Deserialize)]
struct OwnedDesktopSnapshot {
    lifecycle: String,
    arrangement: Vec<(DesktopObject, Placement)>,
}

fn canvas(output: &str, x: f64) -> Placement {
    Placement::Canvas {
        output: output.to_owned(),
        x,
        y: 20.0,
    }
}

fn frame(output: &str) -> Placement {
    Placement::Frame {
        output: output.to_owned(),
        position: 0.5,
    }
}

#[test]
fn one_external_ledger_can_place_duplicate_kinds_alongside_files_and_windows() {
    let mut desktop = Desktop::new();
    desktop
        .arrangement
        .insert(DesktopObject::FileIcon(1), canvas("output-a", 30.0));
    desktop.arrangement.insert(
        DesktopObject::ApplicationWindow(2),
        canvas("output-b", 40.0),
    );
    let first = desktop
        .add_widget(WidgetKind::Clock, canvas("output-a", 100.0))
        .unwrap();
    let second = desktop
        .add_widget(WidgetKind::Clock, frame("output-a"))
        .unwrap();
    let third = desktop
        .add_widget(WidgetKind::Clock, canvas("output-b", 200.0))
        .unwrap();
    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_eq!(desktop.widgets.instances().count(), 3);
    assert_eq!(desktop.arrangement.len(), 5);
    desktop.remove_widget(second).unwrap();
    assert_eq!(desktop.arrangement.len(), 4);
    assert_eq!(
        desktop.arrangement.get(&DesktopObject::FileIcon(1)),
        Some(&canvas("output-a", 30.0))
    );
    assert_eq!(
        desktop
            .arrangement
            .get(&DesktopObject::ApplicationWindow(2)),
        Some(&canvas("output-b", 40.0))
    );
    assert!(desktop.widgets.get(first).is_some());
    assert!(desktop.widgets.get(third).is_some());
}

#[test]
fn canvas_frame_output_moves_preserve_identity_and_preferences() {
    let mut desktop = Desktop::new();
    let id = desktop
        .add_widget(WidgetKind::Clock, canvas("output-a", 100.0))
        .unwrap();
    let preferences = Preferences {
        hide_when_unavailable: false,
        expanded_details: true,
        clock_date_format: Some("%F".to_owned()),
    };
    desktop
        .widgets
        .set_preferences(id, preferences.clone())
        .unwrap();
    desktop.move_widget(id, frame("output-b")).unwrap();
    desktop.move_widget(id, canvas("output-c", -10.0)).unwrap();
    assert_eq!(desktop.widgets.get(id).unwrap().id(), id);
    assert_eq!(desktop.widgets.get(id).unwrap().preferences(), &preferences);
    assert_eq!(desktop.widgets.instances().count(), 1);
    assert_eq!(desktop.arrangement.len(), 1);
    assert_eq!(
        desktop.arrangement.get(&DesktopObject::Widget(id)),
        Some(&canvas("output-c", -10.0))
    );
}

#[test]
fn disconnected_output_arrangements_survive_restart_and_retired_ids_cannot_target_replacements() {
    let mut desktop = Desktop::new();
    let retired = desktop
        .add_widget(WidgetKind::Battery, frame("disconnected-output"))
        .unwrap();
    let kept = desktop
        .add_widget(WidgetKind::Battery, canvas("also-disconnected", 50.0))
        .unwrap();
    desktop.remove_widget(retired).unwrap();
    let mut restored = Desktop::restart(&desktop.snapshot());
    let replacement = restored
        .add_widget(WidgetKind::Battery, frame("disconnected-output"))
        .unwrap();
    assert_ne!(retired, replacement);
    let before = restored.snapshot();
    assert_eq!(
        restored.remove_widget(retired),
        Err(ArrangementError::Lifecycle(Error::UnknownInstance(retired)))
    );
    assert_eq!(
        restored.move_widget(retired, frame("wrong-output")),
        Err(ArrangementError::Lifecycle(Error::UnknownInstance(retired)))
    );
    assert_eq!(restored.snapshot(), before);
    assert_eq!(
        restored.arrangement.get(&DesktopObject::Widget(kept)),
        Some(&canvas("also-disconnected", 50.0))
    );
    assert!(
        restored
            .arrangement
            .contains_key(&DesktopObject::Widget(replacement))
    );
}

#[test]
fn invalid_geometry_is_rejected_by_the_owner_before_either_state_changes() {
    let mut desktop = Desktop::new();
    let id = desktop
        .add_widget(WidgetKind::Clock, canvas("output-a", 100.0))
        .unwrap();
    let before = desktop.snapshot();
    assert_eq!(
        desktop.add_widget(WidgetKind::Clock, canvas("output-a", f64::NAN)),
        Err(ArrangementError::InvalidGeometry)
    );
    assert_eq!(
        desktop.move_widget(id, canvas("output-a", f64::INFINITY)),
        Err(ArrangementError::InvalidGeometry)
    );
    assert_eq!(desktop.snapshot(), before);
}

#[test]
fn missing_external_placement_does_not_partially_retire_an_instance() {
    let mut desktop = Desktop::new();
    let id = desktop
        .widgets
        .add(WidgetKind::Clock, Preferences::default())
        .unwrap();
    let before = desktop.widgets.to_json().unwrap();
    assert_eq!(
        desktop.remove_widget(id),
        Err(ArrangementError::MissingPlacement)
    );
    assert_eq!(desktop.widgets.to_json().unwrap(), before);
    assert!(desktop.widgets.get(id).is_some());
}
