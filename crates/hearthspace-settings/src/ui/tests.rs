use crate::{
    ai::{AiSettings, Operation},
    favorites::tests::fixture,
    state::{BackgroundPickerRequest, SettingsApp, SettingsCategory},
    test_support::{Ui, ids},
};
use hearthspace_ipc::Settings;
use masonry::{
    accesskit::Role,
    core::{
        TextEvent, WidgetId,
        keyboard::{Key, NamedKey},
    },
};
use xilem::{WidgetView, tokio::sync::mpsc::unbounded_channel, view::sized_box};

pub(super) fn state() -> (tempfile::TempDir, SettingsApp) {
    let (dir, favorites) = fixture();
    (
        dir,
        SettingsApp {
            running: true,
            appearance_theme: Default::default(),
            navigation: Default::default(),
            frame_settings: Default::default(),
            ai: AiSettings::default(),
            favorites,
            settings: Settings::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Appearance,
            selected_monitor_id: None,
            background_preview: Default::default(),
            background_picker_request_sender: None,
            background_picker_pending: false,
            activation_listener: None,
            status: "Connected to the settings service".into(),
        },
    )
}

pub(super) fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(super::settings_view(state))
}

pub(super) fn button_starting_with<V: WidgetView<SettingsApp>>(ui: &Ui<V>, name: &str) -> WidgetId
where
    V::Widget: Sized,
{
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Button
                    && node.label().is_some_and(|label| label.contains(name))
            })
        })
        .unwrap_or_else(|| panic!("missing button: {name}"))
}

fn controls_fit_width<V: WidgetView<SettingsApp>>(ui: &Ui<V>, width: f64)
where
    V::Widget: Sized,
{
    for id in ids(&ui.harness) {
        let Some(node) = ui.harness.access_node(id) else {
            continue;
        };
        if matches!(
            node.role(),
            Role::Button
                | Role::CheckBox
                | Role::Switch
                | Role::RadioButton
                | Role::EditableComboBox
                | Role::Canvas
        ) {
            let widget = ui.harness.get_widget_with_id(id);
            let bounds = widget
                .ctx()
                .window_transform()
                .transform_rect_bbox(widget.ctx().border_box());
            assert!(
                bounds.x0 >= 0.0 && bounds.x1 <= width,
                "{:?} is clipped horizontally: {bounds:?}",
                node.label()
            );
        }
    }
}

#[test]
fn appearance_choices_expose_selection_and_live_updates_keep_keyboard_focus() {
    use hearthspace_ipc::{AccentColor, ColorMode};
    use masonry::accesskit::Toggled;
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (680, 490));
    controls_fit_width(&ui, 680.0);
    let light = button_starting_with(&ui, "Light");
    let dark = button_starting_with(&ui, "Dark");
    let blue = button_starting_with(&ui, "Blue");
    for name in [
        "Light", "Dark", "Orange", "Rose", "Violet", "Blue", "Teal", "Olive",
    ] {
        let node = ui
            .harness
            .access_node(button_starting_with(&ui, name))
            .unwrap();
        assert!(!node.is_disabled(), "{name} must remain selectable");
        assert_eq!(node.role(), Role::Button);
        assert_eq!(
            node.toggled(),
            Some(if matches!(name, "Light" | "Orange") {
                Toggled::True
            } else {
                Toggled::False
            })
        );
    }
    ui.harness.focus_on(Some(blue));
    let before = ui.harness.render();
    // This is the same state boundary as a daemon refresh or completed save.
    ui.state.settings.appearance.color_mode = ColorMode::Dark;
    ui.state.settings.appearance.accent_color = AccentColor::Blue;
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(blue));
    assert_eq!(button_starting_with(&ui, "Light"), light);
    assert_eq!(button_starting_with(&ui, "Dark"), dark);
    assert_eq!(
        ui.harness.access_node(light).unwrap().toggled(),
        Some(Toggled::False)
    );
    assert_eq!(
        ui.harness.access_node(dark).unwrap().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(
        ui.harness.access_node(blue).unwrap().toggled(),
        Some(Toggled::True)
    );
    assert!(
        before != ui.harness.render(),
        "live appearance must repaint the same widget tree"
    );
}

#[test]
fn sidebar_routes_to_existing_background_gestures_ai_and_system_controls() {
    let (_dir, mut state) = state();
    let (picker, mut picker_requests) = unbounded_channel();
    state.background_picker_request_sender = Some(picker);
    let (ai, mut ai_requests) = unbounded_channel();
    state.ai.sender = Some(ai);
    let mut ui = Ui::new(state, view, (620, 420));

    ui.activate(ui.button("Background"));
    ui.dispatch(&view);
    controls_fit_width(&ui, 620.0);
    let upload = ui
        .harness
        .get_widget_with_id(ui.button("Upload photo"))
        .ctx()
        .bounding_box();
    assert!(upload.width() < 180.0, "photo action stretches: {upload:?}");
    ui.activate(ui.button("Upload photo"));
    ui.dispatch(&view);
    assert!(matches!(
        picker_requests.try_recv(),
        Ok(BackgroundPickerRequest::PickFile)
    ));
    ui.button("Loading photo…");

    ui.activate(ui.button("Gestures"));
    ui.dispatch(&view);
    controls_fit_width(&ui, 620.0);
    ui.button("Add gesture");

    ui.activate(ui.button("AI"));
    ui.dispatch(&view);
    controls_fit_width(&ui, 620.0);
    assert_eq!(ai_requests.try_recv().unwrap(), Operation::Refresh);
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.role() == Role::Switch && node.label().as_deref() == Some("Enable AI features")
        })
    }));

    ui.activate(ui.button("System"));
    ui.dispatch(&view);
    controls_fit_width(&ui, 620.0);
    ui.button("Change layout");
    ui.button("Restore canvas tools");
    assert!(!ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            matches!(
                node.label().as_deref(),
                Some("Diagnostics" | "Test setting")
            )
        })
    }));
    ui.activate(ui.button("Displays"));
    ui.dispatch(&view);
    controls_fit_width(&ui, 620.0);
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.role() == Role::Canvas
                && node.label().as_deref()
                    == Some("Drag monitor rectangles to configure their arrangement")
        })
    }));
}

#[test]
fn keyboard_navigation_reaches_categories_and_every_appearance_option() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (680, 490));
    ui.harness.focus_on(Some(ui.button("Appearance")));
    ui.harness.press_tab_key(false);
    assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Displays")));
    ui.harness.press_tab_key(false);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Background"))
    );
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    ui.button("Upload photo");

    ui.activate(ui.button("Appearance"));
    ui.dispatch(&view);
    ui.harness
        .focus_on(Some(button_starting_with(&ui, "Light")));
    for name in ["Dark", "Orange", "Rose", "Violet", "Blue", "Teal", "Olive"] {
        ui.harness.press_tab_key(false);
        assert_eq!(
            ui.harness.focused_widget_id(),
            Some(button_starting_with(&ui, name)),
            "Tab must reach {name}"
        );
    }
    // Masonry includes the scroll view and its scrollbar in keyboard navigation.
    for _ in 0..4 {
        ui.harness.press_tab_key(false);
        let focused = ui.harness.focused_widget_id().unwrap();
        if focused == ui.button("Appearance") {
            break;
        }
        let node = ui.harness.access_node(focused).unwrap();
        assert!(
            matches!(node.role(), Role::ScrollBar | Role::ScrollView),
            "unexpected focus: {:?} {:?}",
            node.role(),
            node.label()
        );
    }
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(ui.button("Appearance"))
    );
}

#[test]
fn compact_gesture_recording_keeps_the_pad_and_save_controls_visible_together() {
    let (_dir, mut state) = state();
    state.selected_category = SettingsCategory::Gestures;
    state.favorites.start("Alpha.desktop");
    let ui = Ui::new(state, view, (620, 490));
    controls_fit_width(&ui, 620.0);
    for name in ["Save shortcut", "Back to gestures"] {
        let bounds = ui
            .harness
            .get_widget_with_id(ui.button(name))
            .ctx()
            .bounding_box();
        assert!(bounds.y0 >= 0.0 && bounds.y1 <= 490.0, "{name}: {bounds:?}");
    }
}

#[test]
fn compact_switch_exposes_checked_state_keyboard_action_and_visible_focus() {
    fn switch(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
        crate::page_components::toggle(
            "Test toggle",
            state.settings.test,
            state.background_picker_pending,
            |state, on| state.settings.test = on,
        )
    }
    let (_dir, mut state) = state();
    state.settings.test = false;
    let mut ui = Ui::new(state, switch, (80, 48));
    let id = ui.harness.root_id();
    let node = ui.harness.access_node(id).unwrap();
    assert_eq!(node.role(), Role::Switch);
    assert_eq!(node.label().as_deref(), Some("Test toggle"));
    assert_eq!(node.toggled(), Some(masonry::accesskit::Toggled::False));
    let unfocused = ui.harness.render();
    ui.harness.focus_on(Some(id));
    assert_ne!(unfocused, ui.harness.render(), "focus must be visible");
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
    ui.dispatch(&switch);
    assert!(ui.state.settings.test);
    assert_eq!(
        ui.harness.access_node(id).unwrap().toggled(),
        Some(masonry::accesskit::Toggled::True)
    );
    ui.state.background_picker_pending = true;
    ui.rebuild(&switch);
    ui.activate(id);
    ui.dispatch(&switch);
    assert!(
        ui.state.settings.test,
        "a disabled switch must not mutate state"
    );
    assert!(ui.harness.access_node(id).unwrap().is_disabled());
}

#[test]
fn client_body_rounds_only_bottom_corners_and_preserves_sidebar_pointer_input() {
    for (width, height) in [(680, 490), (620, 420)] {
        let (_dir, state) = state();
        let mut ui = Ui::new(state, view, (width, height));
        for category in ["Gestures", "Appearance"] {
            ui.harness.mouse_click_on(ui.button(category), None);
            ui.dispatch(&view);
            assert_eq!(ui.state.selected_category.label(), category);
            let image = ui.harness.render();
            for x in [2, width - 3] {
                assert_eq!(
                    image.get_pixel(x, height - 3).0[3],
                    0,
                    "bottom corners must reveal the desktop: {category}, {width}x{height}"
                );
                assert!(
                    image.get_pixel(x, 2).0[3] > 128,
                    "top corners must join the server titlebar without a gap"
                );
            }
            assert!(
                (128..220).contains(&image.get_pixel(width / 2, height - 2).0[3]),
                "the Settings body reveals the compositor's blurred backdrop"
            );
        }
    }
}

#[test]
fn every_settings_page_fits_the_compact_minimum_and_desktop_breakpoint() {
    for width in [400, 560] {
        let (_dir, state) = state();
        let mut ui = Ui::new(state, view, (width, 420));
        for category in [
            "Appearance",
            "Displays",
            "Background",
            "Gestures",
            "AI",
            "System",
        ] {
            if width < 560 {
                ui.activate(ui.button("Settings categories"));
                ui.dispatch(&view);
            }
            ui.activate(ui.button(category));
            ui.dispatch(&view);
            controls_fit_width(&ui, f64::from(width));
        }
    }
}
