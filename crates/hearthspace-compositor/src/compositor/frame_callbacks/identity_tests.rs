//! Real protocol metadata feeds App identity queries; no renderer or services start.
use hearthspace_common::{desktop_item_app_id, frame_layout::*};
use smithay::{backend::renderer::utils::with_renderer_surface_state, utils::Rectangle};

use super::test_support::app::AppFixture;
use crate::compositor::{
    ManagedWindowKind,
    layout_editor::{PlacedTarget, Target},
    windows::{RetirementCause, identity::FrameKind, window_app_id},
};

#[test]
fn cached_identity_moves_with_managed_window_and_obeys_current_desktop_kind() {
    let mut f = AppFixture::new();
    let geometry = Rectangle::new((0, 0).into(), (100, 108).into());
    let first = f.window(ManagedWindowKind::DesktopItem, None, geometry);
    let second = f.window(ManagedWindowKind::DesktopItem, None, geometry);
    f.set_app_id(first, &desktop_item_app_id("file:61"));
    f.set_app_id(second, &desktop_item_app_id("file:62"));
    let first_id = f.app.windows[first].id;
    let second_id = f.app.windows[second].id;
    assert_eq!(f.app.desktop_id(first).as_deref(), Some("file:61"));
    assert_eq!(f.app.desktop_id(second).as_deref(), Some("file:62"));

    f.app
        .reclassify_window(second_id, ManagedWindowKind::DesktopItem, None);
    let second_index = f.app.windows.index(second_id).unwrap();
    assert_ne!(second_index, second);
    assert_eq!(f.app.desktop_id(second_index).as_deref(), Some("file:62"));
    let first_index = f.app.windows.index(first_id).unwrap();
    assert_eq!(f.app.desktop_id(first_index).as_deref(), Some("file:61"));

    // Syntax alone must not grant the DesktopItem classification.
    f.app
        .reclassify_window(second_id, ManagedWindowKind::Normal, None);
    assert!(
        f.app
            .desktop_id(f.app.windows.index(second_id).unwrap())
            .is_none()
    );
    f.app
        .reclassify_window(second_id, ManagedWindowKind::DesktopItem, None);
    f.app.retire_window(first_id, RetirementCause::Destroyed);
    assert!(f.app.windows.index(first_id).is_none());
    let survivor = f.app.windows.index(second_id).unwrap();
    assert_eq!(f.app.desktop_id(survivor).as_deref(), Some("file:62"));
    f.set_app_id(second, &desktop_item_app_id("file:63"));
    assert_eq!(f.app.desktop_id(survivor).as_deref(), Some("file:63"));
}

#[test]
fn cached_frame_syntax_does_not_freeze_placement_or_mapping_eligibility() {
    let mut f = AppFixture::new();
    let index = f.window(
        ManagedWindowKind::ShellBar,
        Some("A"),
        Rectangle::new((0, 0).into(), (44, 44).into()),
    );
    f.set_app_id(index, &frame_widget_app_id(Some("A"), "wifi"));
    let target = Target {
        output: "A".into(),
        target: LayoutTarget::widget("wifi"),
    };
    assert_eq!(f.app.frame_target_for_window(index), Some(target.clone()));
    assert!(!f.app.layout_surface_visible(index));
    f.app.layout_editor.placed.push(PlacedTarget {
        target,
        rect: Rect {
            x: 20.,
            y: 20.,
            width: 44.,
            height: 44.,
        },
        placement: ObjectPlacement::canvas(),
        group: None,
        vertical: false,
    });
    let output = f.app.output_logical_rect_for_name(Some("A")).unwrap();
    assert!(f.app.window_visible_on_output(index, output));
    f.app.layout_editor.placed[0].placement = ObjectPlacement::Hidden;
    assert!(!f.app.layout_surface_visible(index));
    f.app.layout_editor.placed[0].placement = ObjectPlacement::canvas();
    assert!(f.app.layout_surface_visible(index));
    f.app.windows[index].mapped = false;
    assert!(!f.app.window_visible_on_output(index, output));
    f.app.windows[index].mapped = true;
    assert!(f.app.window_visible_on_output(index, output));
    f.app.layout_editor.placed.clear();
    assert!(!f.app.layout_surface_visible(index));
}

#[rstest::rstest]
#[case::widget(FrameKind::Widget)]
#[case::group(FrameKind::Group)]
#[case::grip(FrameKind::Grip)]
fn cached_frame_identity_observes_protocol_role_reset_without_app_id_changed(
    #[case] kind: FrameKind,
) {
    let mut f = AppFixture::new();
    let index = f.window(
        ManagedWindowKind::ShellBar,
        Some("A"),
        Rectangle::new((0, 0).into(), (44, 44).into()),
    );
    let (app_id, target) = match kind {
        FrameKind::Widget => (frame_widget_app_id(Some("A"), "wifi"), "wifi"),
        FrameKind::Group => (frame_group_app_id(Some("A"), "system"), "system"),
        FrameKind::Grip => (frame_group_grip_app_id(Some("A"), "system"), "system"),
    };
    let root = f.app.windows[index].surface.wl_surface().unwrap();
    let has_buffer =
        || with_renderer_surface_state(&root, |state| state.buffer().is_some()).unwrap_or(false);
    f.set_app_id(index, &app_id);
    f.commit_buffer(index);
    assert!(
        has_buffer(),
        "a real buffer must precede the null-buffer reset"
    );
    let expected = Some((Some("A".into()), target.into()));
    assert_eq!(f.app.windows[index].frame_identity(kind), expected);
    assert!(f.app.is_layout_surface(index));
    assert!(f.app.frame_target_for_window(index).is_some());

    f.unmap_buffer(index);
    assert!(!has_buffer());
    assert_eq!(window_app_id(&f.app.windows[index].surface), None);
    // Managed mapping stays fixture input. Query syntax directly so neither
    // mapping nor layout_surface_visible's raw-ID guard can mask stale syntax.
    assert!(f.app.windows[index].mapped);
    assert_eq!(f.app.windows[index].frame_identity(kind), None);
    assert!(!f.app.is_layout_surface(index));
    assert_eq!(f.app.frame_target_for_window(index), None);

    f.commit_buffer(index);
    assert!(has_buffer());
    assert_eq!(window_app_id(&f.app.windows[index].surface), None);
    assert_eq!(f.app.windows[index].frame_identity(kind), None);
    assert!(!f.app.is_layout_surface(index));
    assert_eq!(f.app.frame_target_for_window(index), None);
    f.set_app_id(index, &app_id);
    assert_eq!(f.app.windows[index].frame_identity(kind), expected);
    assert!(f.app.is_layout_surface(index));
    assert!(f.app.frame_target_for_window(index).is_some());
}

#[cfg(feature = "udev")]
#[path = "identity_tests/outputs.rs"]
mod outputs;
