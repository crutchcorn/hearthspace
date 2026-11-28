use hearthspace_common::frame_layout::{
    ClockDateFormat, FrameEdge, FrameLayout, LayoutTarget, ObjectPlacement, TargetKind,
};

fn canvas(x: f64) -> ObjectPlacement {
    ObjectPlacement::Canvas {
        x: Some(x),
        y: Some(200.0),
        width: None,
        height: None,
    }
}

#[test]
fn duplicate_widgets_move_group_remove_and_restart_independently() {
    let mut layout = FrameLayout::default();
    let starter = layout.widget_id("clock").unwrap();
    let first = layout.add_widget("clock", canvas(200.0)).unwrap();
    let second = layout.add_widget("clock", canvas(400.0)).unwrap();
    layout
        .set_widget_clock_date_format(&first, ClockDateFormat::Long)
        .unwrap();
    layout
        .set_widget_clock_date_format(&second, ClockDateFormat::Numeric)
        .unwrap();
    let first_target = LayoutTarget::widget(&first);
    layout.set_placement(&first_target, ObjectPlacement::frame(FrameEdge::Left, 0.5));
    let file = LayoutTarget::new(TargetKind::Item, "file:61");
    let group = layout.merge(&file, &first_target, false).unwrap();
    layout.remove_widget(&first).unwrap();
    assert!(!layout.groups.contains_key(&group.id));
    assert_eq!(
        layout.placement(&file),
        ObjectPlacement::frame(FrameEdge::Left, 0.5)
    );
    assert_eq!(
        layout.placement(&LayoutTarget::widget(&second)),
        canvas(400.0)
    );
    assert_eq!(
        layout.widget_clock_date_format(&second),
        ClockDateFormat::Numeric
    );
    assert!(layout.widget_kind(&starter).is_some());
    assert!(layout.widget_kind(&first).is_none());
    assert_eq!(layout.validate(), Ok(()));

    let encoded = serde_json::to_string(&layout).unwrap();
    let mut restored: FrameLayout = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, layout);
    assert_eq!(restored.remove_widget(&first), Err("widget disappeared"));
    let replacement = restored.add_widget("clock", canvas(250.0)).unwrap();
    assert_ne!(replacement, first);
    assert_eq!(
        restored.widget_clock_date_format(&replacement),
        ClockDateFormat::None
    );
    assert_eq!(restored.validate(), Ok(()));
}

#[test]
fn removing_canvas_tools_and_all_other_copies_stays_empty_after_restart() {
    let mut layout = FrameLayout::default();
    let targets: Vec<_> = layout.widget_targets().collect();
    for target in targets {
        layout.set_placement(&target, canvas(120.0));
        layout.set_placement(&target, ObjectPlacement::Hidden);
        assert!(layout.widget_kind(&target.id).is_none());
    }
    assert_eq!(layout.widgets.instances().count(), 0);
    assert!(layout.groups.is_empty());
    let encoded = serde_json::to_string(&layout).unwrap();
    let restored: FrameLayout = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored.widgets.instances().count(), 0);
    assert_eq!(restored.validate(), Ok(()));
}

#[test]
fn legacy_saved_layout_migrates_controls_groups_preferences_and_removal() {
    let saved = serde_json::json!({
        "placements": {
            "widget:notifications": {"surface":"canvas","x":250.0,"y":300.0},
            "widget:power": {"surface":"hidden"},
            "group:system": {"surface":"frame","edge":"right","position":0.25}
        },
        "groups": {"system":{"id":"system","members":[
            {"kind":"widget","id":"clock"},{"kind":"widget","id":"volume"}
        ]}},
        "expanded":{"volume":true}, "clock_date_format":"long"
    });
    let layout: FrameLayout = serde_json::from_value(saved).unwrap();
    let notifications = layout.widget_id("notifications").unwrap();
    assert_eq!(
        layout
            .placement(&LayoutTarget::widget(notifications))
            .canvas_bounds()
            .unwrap()
            .0,
        Some(250.0)
    );
    assert!(layout.widget_id("power").is_none());
    let clock = layout.widget_id("clock").unwrap();
    assert_eq!(
        layout.widget_clock_date_format(&clock),
        ClockDateFormat::Long
    );
    assert_eq!(
        layout.placement(&LayoutTarget::widget(&clock)),
        ObjectPlacement::frame(FrameEdge::Right, 0.25)
    );
    assert!(layout.is_expanded(&layout.widget_id("volume").unwrap()));
    assert_eq!(layout.validate(), Ok(()));
    let restored: FrameLayout =
        serde_json::from_str(&serde_json::to_string(&layout).unwrap()).unwrap();
    assert_eq!(restored, layout);
}

#[test]
fn invalid_widget_add_is_atomic_and_cannot_register_an_unplaced_copy() {
    let mut layout = FrameLayout::default();
    let before = layout.clone();
    assert!(layout.add_widget("clock", canvas(f64::NAN)).is_err());
    assert!(layout.add_widget("unknown", canvas(100.0)).is_err());
    assert!(layout.add_widget("clock", ObjectPlacement::Hidden).is_err());
    assert_eq!(layout, before);
}

#[test]
fn legacy_conflicting_ownership_is_rejected_before_migration_can_discard_a_placement() {
    let saved = serde_json::json!({
        "placements":{"widget:clock":{"surface":"canvas","x":42.0,"y":84.0}},
        "groups":{"system":{"id":"system","members":[
            {"kind":"widget","id":"clock"},{"kind":"widget","id":"volume"}
        ]}}
    });
    assert!(serde_json::from_value::<FrameLayout>(saved).is_err());
}
