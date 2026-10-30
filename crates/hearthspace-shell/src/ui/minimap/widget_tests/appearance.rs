use super::*;
use hearthspace_common::{AccentColor, Appearance, ColorMode};
use hearthspace_ui_components::Palette;
use masonry::testing::TestHarnessParams;
use std::sync::Arc;

#[test]
fn map_border_geometry_and_keyboard_focus_ring_keep_accessible_identity() {
    let mut harness = TestHarness::create_with(
        crate::ui::appearance::properties(Palette::default()),
        Minimap::new(Some(snapshot(1.0)), Some("HDMI-A-1".into())).prepare(),
        TestHarnessParams::default()
            .with_size((310, 168))
            .with_padding(8),
    );
    let idle = harness.render();
    let id = harness.root_id();
    let node = harness.access_node(id).unwrap();
    assert_eq!(node.role(), Role::Image);
    let bounds = node.bounding_box().unwrap();
    assert_eq!(bounds.width(), 310.0);
    assert_eq!(bounds.height(), 168.0);
    assert!(node.description().unwrap().contains("center the viewport"));
    assert_ne!(
        idle.get_pixel(8, 8),
        idle.get_pixel(163, 8),
        "the rounded corner must not paint a square border"
    );
    harness.press_tab_key(false);
    assert_eq!(harness.focused_widget_id(), Some(id));
    let focused = harness.render();
    assert_ne!(
        idle.get_pixel(163, 4),
        focused.get_pixel(163, 4),
        "keyboard focus paints a two-pixel ring three pixels outside the border"
    );
    assert_eq!(
        harness.access_node(id).unwrap().bounding_box(),
        Some(bounds)
    );
    assert!(
        drain(&mut harness).is_empty(),
        "focusing the image never navigates"
    );
    harness.mouse_move(center(&harness));
    harness.mouse_button_press(Some(PointerButton::Primary));
    let pointer = harness.render();
    assert_ne!(
        focused.get_pixel(163, 4),
        pointer.get_pixel(163, 4),
        "pointer focus hides the keyboard ring"
    );
    assert_eq!(harness.focused_widget_id(), Some(id));
}

#[test]
fn focused_window_metadata_changes_without_a_second_accent_selection() {
    let mut harness = harness(1.0);
    let before = harness.render();
    harness.edit_root_widget(|mut root| {
        let window = &mut root.widget.snapshot.as_mut().unwrap().windows[0];
        window.focused = true;
        window.title = "Notes".into();
        root.ctx.request_render();
        root.ctx.request_accessibility_update();
    });
    assert_eq!(
        before,
        harness.render(),
        "window focus must not compete with the selected viewport's appearance"
    );
    assert!(
        harness
            .access_node(harness.root_id())
            .unwrap()
            .label()
            .unwrap()
            .contains("Focused window: Notes")
    );
}

#[test]
fn live_palette_repaints_map_without_replacing_drag_focus_or_projection() {
    let mut harness = harness(1.0);
    let start = center(&harness);
    harness.mouse_move(start);
    harness.mouse_button_press(Some(PointerButton::Primary));
    assert_eq!(drain(&mut harness), [MapAction::Begin]);
    let id = harness.focused_widget_id().unwrap();
    let bounds = harness.access_node(id).unwrap().bounding_box();
    let projection = harness
        .root_widget()
        .drag
        .as_ref()
        .unwrap()
        .projection
        .clone();
    let original = harness.root_widget().snapshot.clone();
    let light = harness.render();
    let palette = Palette::new(Appearance {
        color_mode: ColorMode::Dark,
        accent_color: AccentColor::Teal,
    });
    harness.set_default_properties(Arc::new(crate::ui::appearance::properties(palette)));
    assert_ne!(light, harness.render());
    assert_eq!(harness.focused_widget_id(), Some(id));
    assert_eq!(harness.access_node(id).unwrap().bounding_box(), bounds);
    assert_eq!(harness.root_widget().snapshot, original);
    assert_eq!(
        harness
            .root_widget()
            .drag
            .as_ref()
            .unwrap()
            .projection
            .world,
        projection.world
    );
    assert_eq!(
        harness
            .root_widget()
            .drag
            .as_ref()
            .unwrap()
            .projection
            .scale,
        projection.scale
    );
    harness.mouse_move(start + (15.0, -20.0));
    assert!(matches!(
        drain(&mut harness).as_slice(),
        [MapAction::PanTo { .. }]
    ));
    harness.mouse_button_release(Some(PointerButton::Primary));
    assert_eq!(drain(&mut harness), [MapAction::End]);
}
