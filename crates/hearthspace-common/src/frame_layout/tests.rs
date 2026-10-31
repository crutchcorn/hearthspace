use super::*;

const VIEW: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 1280.0,
    height: 800.0,
};

#[test]
fn separate_move_and_regroup_preserves_members_and_destination_order() {
    let mut layout = FrameLayout::default();
    let wifi = LayoutTarget::widget("wifi");
    layout.set_placement(
        &wifi,
        ObjectPlacement::Canvas {
            x: Some(400.0),
            y: Some(300.0),
            width: Some(120.0),
            height: Some(54.0),
        },
    );
    assert!(layout.group_for(&wifi).is_none());
    assert!(!layout.groups["system"].members.contains(&wifi));
    layout.set_placement(
        &wifi,
        ObjectPlacement::Canvas {
            x: Some(500.0),
            y: Some(200.0),
            width: None,
            height: None,
        },
    );
    assert_eq!(
        layout.placement(&wifi).canvas_bounds(),
        Some((Some(500.0), Some(200.0), Some(120.0), Some(54.0)))
    );
    let group = layout
        .merge(&wifi, &LayoutTarget::widget("volume"), true)
        .unwrap();
    assert_eq!(group.id, "system");
    assert_eq!(layout.groups["system"].members[1], wifi);
    assert_eq!(
        layout.placement(&wifi),
        ObjectPlacement::frame(FrameEdge::Top, 1.0)
    );
    assert_eq!(layout.validate(), Ok(()));
}

#[test]
fn mixed_groups_dissolve_without_hiding_apps_or_losing_the_tools() {
    let mut layout = FrameLayout::default();
    let tools = LayoutTarget::widget("tools");
    let window = LayoutTarget::new(TargetKind::Window, "42");
    let item = LayoutTarget::new(TargetKind::Item, "app:terminal");
    let group = layout.merge(&window, &tools, false).unwrap();
    layout.merge(&item, &group, false).unwrap();
    layout.set_placement(&group, ObjectPlacement::Hidden);
    assert_eq!(
        layout.placement(&tools),
        ObjectPlacement::frame(FrameEdge::Bottom, 0.0)
    );
    assert_eq!(layout.placement(&window), ObjectPlacement::canvas());
    assert_eq!(layout.placement(&item), ObjectPlacement::background());
    assert!(layout.group_for(&tools).is_none());
    assert_eq!(layout.validate(), Ok(()));
    layout.set_allow_hide_tools(true);
    layout.set_placement(&tools, ObjectPlacement::Hidden);
    assert_eq!(layout.placement(&tools), ObjectPlacement::Hidden);
    layout.set_allow_hide_tools(false);
    assert!(layout.placement(&tools).is_frame());
}

#[test]
fn closing_grouped_window_leaves_remaining_member_at_same_edge() {
    let mut layout = FrameLayout::default();
    let notifications = LayoutTarget::widget("notifications");
    let window = LayoutTarget::new(TargetKind::Window, "42");
    layout.merge(&window, &notifications, false).unwrap();
    layout.remove_target(&window);
    assert!(layout.group_for(&notifications).is_none());
    assert_eq!(
        layout.placement(&notifications),
        ObjectPlacement::frame(FrameEdge::Top, 0.0)
    );
    assert_eq!(layout.validate(), Ok(()));
}

#[test]
fn layout_round_trip_retains_hidden_widgets_groups_and_clock_options() {
    let mut layout = FrameLayout::default();
    layout.set_placement(&LayoutTarget::widget("battery"), ObjectPlacement::Hidden);
    layout.clock_date_format = ClockDateFormat::Long;
    layout.toggle_expanded("wifi");
    let saved = serde_json::to_string(&layout).unwrap();
    let restored: FrameLayout = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored, layout);
    assert_eq!(restored.validate(), Ok(()));
    assert!(restored.is_expanded("wifi"));
}

#[test]
fn invalid_layout_cannot_duplicate_members_or_hide_protected_tools() {
    let mut layout = FrameLayout::default();
    layout
        .groups
        .get_mut("system")
        .unwrap()
        .members
        .push(LayoutTarget::widget("clock"));
    assert!(layout.validate().is_err());
    let mut layout = FrameLayout::default();
    layout
        .placements
        .insert("widget:tools".into(), ObjectPlacement::Hidden);
    assert!(layout.validate().is_err());
    assert!(LayoutTarget::from_key("drawing:1").is_none());
    assert!(LayoutTarget::from_key("widget:unknown").is_none());
}

#[test]
fn corner_drop_retains_toolbar_orientation_without_long_bar_false_hits() {
    let toolbar = Rect {
        x: 500.0,
        y: 300.0,
        width: 600.0,
        height: 54.0,
    };
    let corner = Some(Point { x: 30.0, y: 30.0 });
    assert_eq!(
        widget_frame_drop_target(toolbar, VIEW, 72.0, Some(FrameEdge::Right), corner),
        Some(ObjectPlacement::frame(FrameEdge::Left, 0.0))
    );
    assert_eq!(
        widget_frame_drop_target(toolbar, VIEW, 72.0, Some(FrameEdge::Top), corner),
        Some(ObjectPlacement::frame(FrameEdge::Top, 0.0))
    );
    assert_eq!(
        widget_frame_drop_target(
            toolbar,
            VIEW,
            72.0,
            None,
            Some(Point { x: 640.0, y: 400.0 })
        ),
        None
    );
}

#[test]
fn pinned_window_has_exact_recess_hover_and_full_reveal_geometry() {
    let bounds = frame_bounds(
        ObjectPlacement::frame(FrameEdge::Right, 0.5),
        640.0,
        480.0,
        VIEW,
        false,
    );
    let offset = pinned_window_offset(bounds, FrameEdge::Right, VIEW, false, PINNED_EXPOSED_SIZE);
    let visible = frame_drag_bounds(
        Rect {
            x: bounds.x + offset.x,
            ..bounds
        },
        VIEW,
    );
    assert_eq!(
        visible,
        Rect {
            x: 1238.0,
            y: 160.0,
            width: 42.0,
            height: 480.0
        }
    );
    let hover = pinned_window_offset(bounds, FrameEdge::Right, VIEW, false, PINNED_HOVER_SIZE);
    assert_eq!(bounds.x + hover.x, 1180.0);
    assert_eq!(
        pinned_window_offset(bounds, FrameEdge::Right, VIEW, true, 42.0),
        Point::default()
    );
}

#[test]
fn guide_rects_and_fitted_controls_stay_usable_on_compact_outputs() {
    let view = Rect {
        width: 140.0,
        height: 100.0,
        ..Rect::default()
    };
    assert_eq!(guide_size(view), 34.0);
    for (_, rect) in guide_rects(view) {
        assert!(rect.x >= 0.0 && rect.y >= 0.0 && rect.width >= 0.0 && rect.height >= 0.0);
        assert!(rect.x + rect.width <= view.width && rect.y + rect.height <= view.height);
    }
    assert_eq!(
        frame_bounds(
            ObjectPlacement::frame(FrameEdge::Bottom, 1.0),
            500.0,
            54.0,
            view,
            true
        ),
        Rect {
            x: 12.0,
            y: 34.0,
            width: 116.0,
            height: 54.0
        }
    );
}

#[test]
fn frame_snap_uses_global_grid_but_preserves_exact_corners() {
    let size = Rect {
        width: 54.0,
        height: 54.0,
        ..Rect::default()
    };
    let placement = ObjectPlacement::frame(FrameEdge::Top, 0.37);
    let origin = Point { x: 10.0, y: 18.0 };
    let snapped = snap_frame_placement(placement, size, VIEW, origin, 50.0, true);
    let bounds = frame_bounds(snapped, size.width, size.height, VIEW, true);
    assert!(((bounds.x + origin.x) % 50.0).abs() < 0.0001);
    let corner = ObjectPlacement::frame(FrameEdge::Top, 0.0);
    assert_eq!(
        snap_frame_placement(corner, size, VIEW, origin, 50.0, true),
        corner
    );
}

#[test]
fn native_layout_surface_ids_round_trip_unusual_outputs_and_group_names() {
    let output = Some("card0/DP-1 🖥");
    assert_eq!(
        frame_widget_from_app_id(&frame_widget_app_id(output, "wifi")),
        Some((output.map(str::to_string), "wifi".into()))
    );
    assert_eq!(
        frame_group_from_app_id(&frame_group_app_id(output, "one:two /三")),
        Some((output.map(str::to_string), "one:two /三".into()))
    );
    assert_eq!(
        layout_editor_from_app_id(&layout_editor_app_id(42, output)),
        Some((42, output.map(str::to_string)))
    );
    assert_eq!(
        pinned_window_reveal_from_app_id(&pinned_window_reveal_app_id(42)),
        Some(42)
    );
    assert_eq!(
        layout_editor_from_app_id(&layout_editor_app_id(0, output)),
        None
    );
    assert_eq!(
        frame_widget_from_app_id(&frame_widget_app_id(output, "drawing")),
        None
    );
    assert_eq!(
        frame_group_from_app_id(&frame_group_app_id(Some(""), "group")),
        None
    );
}

#[test]
fn native_layout_surface_ids_keep_gpu_qualified_outputs_and_distinct_families() {
    let output = Some("eDP-1@%2Fsys%2Fdevices%2Fpci0000:00%2F0000:00:08.1%2F0000:c6:00.0");
    let targets = [
        frame_widget_app_id(output, "wifi"),
        frame_group_app_id(output, "wifi"),
        frame_group_grip_app_id(output, "wifi"),
    ];
    type TargetIdentity = (Option<String>, String);
    let target_parsers: [fn(&str) -> Option<TargetIdentity>; 3] = [
        frame_widget_from_app_id,
        frame_group_from_app_id,
        frame_group_grip_from_app_id,
    ];
    for (family, parse) in target_parsers.into_iter().enumerate() {
        for (candidate, id) in targets.iter().enumerate() {
            assert_eq!(
                parse(id),
                (family == candidate).then(|| (output.map(str::to_owned), "wifi".into())),
                "target family {family} parsing {id}"
            );
        }
    }
    let sessions = [
        layout_editor_app_id(42, output),
        layout_confirm_app_id(42, output),
        layout_tooltip_app_id(42, output),
    ];
    type SessionIdentity = (u64, Option<String>);
    let session_parsers: [fn(&str) -> Option<SessionIdentity>; 3] = [
        layout_editor_from_app_id,
        layout_confirm_from_app_id,
        layout_tooltip_from_app_id,
    ];
    for (family, parse) in session_parsers.into_iter().enumerate() {
        for (candidate, id) in sessions.iter().enumerate() {
            assert_eq!(
                parse(id),
                (family == candidate).then(|| (42, output.map(str::to_owned))),
                "session family {family} parsing {id}"
            );
        }
    }
}

#[test]
fn native_layout_surface_ids_reject_malformed_targets_and_outputs() {
    // Empty, odd-length, non-hex, invalid UTF-8, newline, and NUL encodings.
    for encoded in ["", "0", "gg", "ff", "610a", "6100"] {
        let target = format!("dev.hearthspace.frame-group.target-{encoded}");
        assert_eq!(frame_group_from_app_id(&target), None, "{target}");
        let output = format!("dev.hearthspace.frame-group.output-{encoded}.target-61");
        assert_eq!(frame_group_from_app_id(&output), None, "{output}");
        let session = format!("dev.hearthspace.layout-editor.output-{encoded}.session-42");
        assert_eq!(layout_editor_from_app_id(&session), None, "{session}");
    }
    // Uppercase hex remains valid, including a default-output target.
    assert_eq!(
        frame_group_from_app_id("dev.hearthspace.frame-group.target-5A"),
        Some((None, "Z".into()))
    );
    assert_eq!(
        layout_editor_from_app_id("dev.hearthspace.layout-editor.output-44502D31.session-42"),
        Some((42, Some("DP-1".into())))
    );
    let output = "x".repeat(256);
    assert_eq!(
        frame_group_from_app_id(&frame_group_app_id(Some(&output), "group")),
        Some((Some(output.clone()), "group".into()))
    );
    let too_long = format!("{output}x");
    assert_eq!(
        frame_group_from_app_id(&frame_group_app_id(Some(&too_long), "group")),
        None
    );
    assert_eq!(
        layout_editor_from_app_id(&layout_editor_app_id(42, Some(&too_long))),
        None
    );
}

#[test]
fn native_layout_surface_ids_reject_zero_sessions_and_extended_family_names() {
    for output in [
        None,
        Some("DP-8@%2Fsys%2Fdevices%2Fpci0000:00%2F0000:c4:00.0"),
    ] {
        assert_eq!(
            layout_editor_from_app_id(&layout_editor_app_id(0, output)),
            None
        );
        assert_eq!(
            layout_confirm_from_app_id(&layout_confirm_app_id(0, output)),
            None
        );
        assert_eq!(
            layout_tooltip_from_app_id(&layout_tooltip_app_id(0, output)),
            None
        );
    }
    for suffix in ["", "-other", ".other"] {
        let target = format!("dev.hearthspace.frame-group{suffix}");
        assert_eq!(frame_group_from_app_id(&target), None);
        if !suffix.is_empty() {
            assert_eq!(
                frame_group_from_app_id(&format!("{target}.target-61")),
                None
            );
            let session = format!("dev.hearthspace.layout-editor{suffix}.session-42");
            assert_eq!(layout_editor_from_app_id(&session), None, "{session}");
        }
    }
    for session in ["", "-1", "NaN", "18446744073709551616"] {
        let id = format!("dev.hearthspace.layout-editor.session-{session}");
        assert_eq!(layout_editor_from_app_id(&id), None, "{id}");
    }
}
