use super::*;
use hearthspace_common::{AccentColor, Appearance, ColorMode};
use xilem::masonry::{
    accesskit::{Action, ActionRequest, Role, Toggled, TreeId},
    core::{
        NewWidget, PointerButton, TextEvent, Widget, WidgetId,
        keyboard::{Key, NamedKey},
    },
    testing::TestHarness,
};

fn tile(name: &str, state: TileState) -> (TestHarness<TileSurface>, WidgetId) {
    let child = NewWidget::new(DesktopTile::new(
        TileContent {
            name: name.into(),
            artwork: Artwork::Settings,
        },
        state,
    ));
    let id = child.id();
    let mut properties = hearthspace_ui_components::hearthspace_light_properties();
    theme_properties(&mut properties, Palette::default());
    let harness =
        TestHarness::create_with_size(properties, TileSurface::new(child).prepare(), (100, 108));
    (harness, id)
}

fn access(harness: &mut TestHarness<TileSurface>, id: WidgetId, action: Action) {
    harness.process_access_event(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: id.into(),
        data: None,
    });
}

#[test]
fn pointer_and_keys_keep_native_focus_without_launching_twice() {
    let (mut harness, id) = tile(
        "Settings",
        TileState {
            selected: true,
            ..TileState::default()
        },
    );
    let node = harness.access_node(id).unwrap();
    assert_eq!(node.role(), Role::Button);
    assert_eq!(node.label().as_deref(), Some("Settings"));
    assert_eq!(node.data().toggled(), Some(Toggled::True));
    let bounds = node.data().bounds().unwrap();
    assert_eq!((bounds.width(), bounds.height()), (88.0, 96.0));

    harness.mouse_click_on(id, Some(PointerButton::Primary));
    assert_eq!(harness.focused_widget_id(), Some(id));
    harness.process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    harness.process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    assert!(
        harness.pop_action::<TileAction>().is_none(),
        "compositor owns ordinary activation"
    );

    access(&mut harness, id, Action::Click);
    assert_eq!(
        harness.pop_action::<TileAction>().unwrap().0,
        TileAction::Activate
    );
    access(&mut harness, id, Action::Focus);
    assert_eq!(
        harness.pop_action::<TileAction>().unwrap().0,
        TileAction::Focus
    );
}

#[test]
fn compositor_focus_updates_restore_the_actual_accessible_button() {
    let (mut harness, id) = tile("Settings", TileState::default());
    harness.edit_root_widget(|mut root| {
        let mut child = TileSurface::child_mut(&mut root);
        DesktopTile::set(
            &mut child,
            TileContent {
                name: "Settings".into(),
                artwork: Artwork::Settings,
            },
            TileState {
                focused: true,
                keyboard_focus: true,
                ..TileState::default()
            },
        );
    });
    assert_eq!(harness.focused_widget_id(), Some(id));
    assert!(
        harness.pop_action::<TileAction>().is_none(),
        "snapshot focus must not echo an IPC action"
    );
    harness.edit_root_widget(|mut root| {
        let mut child = TileSurface::child_mut(&mut root);
        DesktopTile::set(
            &mut child,
            TileContent {
                name: "Settings".into(),
                artwork: Artwork::Settings,
            },
            TileState::default(),
        );
    });
    assert_eq!(harness.focused_widget_id(), None);
}

#[test]
fn long_unicode_names_wrap_then_ellipsize_while_the_full_name_remains_accessible() {
    let name = "超長的桌面應用程式名稱 🪴 A very long desktop application with many words";
    let (mut harness, id) = tile(name, TileState::default());
    harness.render();
    let widget = harness
        .get_widget_with_id(id)
        .downcast::<DesktopTile>()
        .unwrap()
        .inner();
    assert_eq!(widget.caption.layout.len(), 2);
    assert!(widget.caption.layout.width() <= 74.01);
    assert!(widget.caption.displayed.ends_with('…'));
    assert_eq!(
        harness.access_node(id).unwrap().label().as_deref(),
        Some(name)
    );
}

#[test]
fn keyboard_ring_fits_the_transparent_surface_and_pointer_focus_omits_it() {
    let state = TileState {
        selected: true,
        focused: true,
        ..TileState::default()
    };
    let (mut harness, _) = tile("Settings", state);
    let pointer = harness.render();
    harness.edit_root_widget(|mut root| {
        let mut child = TileSurface::child_mut(&mut root);
        DesktopTile::set(
            &mut child,
            TileContent {
                name: "Settings".into(),
                artwork: Artwork::Settings,
            },
            TileState {
                keyboard_focus: true,
                ..state
            },
        );
    });
    let keyboard = harness.render();
    assert_ne!(pointer.get_pixel(50, 3), keyboard.get_pixel(50, 3));
    assert_eq!(
        pointer.get_pixel(50, 0),
        keyboard.get_pixel(50, 0),
        "ring must fit inside margin"
    );
}

#[test]
fn photo_icons_keep_the_base_accent_tint_in_both_color_modes() {
    let light = Palette::new(Appearance {
        accent_color: AccentColor::Blue,
        color_mode: ColorMode::Light,
    });
    let dark = Palette::new(Appearance {
        color_mode: ColorMode::Dark,
        ..light.appearance
    });
    assert_eq!(paint::pale_ink(&light), paint::pale_ink(&dark));
}
