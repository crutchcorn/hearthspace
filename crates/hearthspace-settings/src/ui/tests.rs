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

fn state() -> (tempfile::TempDir, SettingsApp) {
    let (dir, favorites) = fixture();
    (
        dir,
        SettingsApp {
            ai: AiSettings::default(),
            favorites,
            settings: Settings::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Appearance,
            selected_monitor_id: None,
            background_picker_request_sender: None,
            background_picker_pending: false,
            status: "Connected to the settings service".into(),
            config_label: "Config: settings.json".into(),
        },
    )
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(super::settings_view(state))
}

fn button_starting_with<V: WidgetView<SettingsApp>>(ui: &Ui<V>, name: &str) -> WidgetId
where
    V::Widget: Sized,
{
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.role() == Role::Button
                    && node.label().is_some_and(|label| label.starts_with(name))
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
        if matches!(node.role(), Role::Button | Role::CheckBox) {
            let bounds = ui.harness.get_widget_with_id(id).ctx().bounding_box();
            assert!(
                bounds.x0 >= 0.0 && bounds.x1 <= width,
                "{:?} is clipped horizontally: {bounds:?}",
                node.label()
            );
        }
    }
}

#[test]
fn appearance_exposes_current_theme_and_unavailable_choices_without_changing_settings() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (680, 490));
    controls_fit_width(&ui, 680.0);
    let light = button_starting_with(&ui, "Light");
    let light_node = ui.harness.access_node(light).unwrap();
    assert!(light_node.label().unwrap().contains("Current"));
    assert!(!light_node.is_disabled());
    let orange = button_starting_with(&ui, "Orange");
    assert!(
        ui.harness
            .access_node(orange)
            .unwrap()
            .label()
            .unwrap()
            .contains("Default")
    );

    for name in ["Dark", "Rose", "Violet", "Blue", "Teal", "Olive"] {
        let choice = button_starting_with(&ui, name);
        let node = ui.harness.access_node(choice).unwrap();
        assert!(
            node.is_disabled(),
            "{name} must not offer an unsupported setting"
        );
        assert!(node.label().unwrap().contains("Unavailable"));
        ui.activate(choice);
        ui.dispatch(&view);
        assert_eq!(ui.state.selected_category, SettingsCategory::Appearance);
    }
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
    ui.activate(ui.button("Select background image"));
    ui.dispatch(&view);
    assert!(matches!(
        picker_requests.try_recv(),
        Ok(BackgroundPickerRequest::PickFile)
    ));
    ui.button("Selecting background image...");

    ui.activate(ui.button("Gestures"));
    ui.dispatch(&view);
    controls_fit_width(&ui, 620.0);
    ui.button("Reload gestures and apps");

    ui.activate(ui.button("AI"));
    ui.dispatch(&view);
    controls_fit_width(&ui, 620.0);
    assert_eq!(ai_requests.try_recv().unwrap(), Operation::Refresh);
    ui.button("Download D1 model");

    ui.activate(ui.button("System"));
    ui.dispatch(&view);
    controls_fit_width(&ui, 620.0);
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.role() == Role::CheckBox && node.label().as_deref() == Some("Test setting")
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
fn keyboard_navigation_reaches_categories_and_skips_unavailable_appearance_options() {
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
    ui.button("Select background image");

    ui.activate(ui.button("Appearance"));
    ui.dispatch(&view);
    ui.harness
        .focus_on(Some(button_starting_with(&ui, "Light")));
    ui.harness.press_tab_key(false);
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(button_starting_with(&ui, "Orange"))
    );
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
    for name in ["Save gesture", "Try again", "Cancel recording"] {
        let bounds = ui
            .harness
            .get_widget_with_id(ui.button(name))
            .ctx()
            .bounding_box();
        assert!(bounds.y0 >= 0.0 && bounds.y1 <= 490.0, "{name}: {bounds:?}");
    }
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
                assert_eq!(
                    image.get_pixel(x, 2).0[3],
                    255,
                    "top corners must join the server titlebar without a gap"
                );
            }
            assert_eq!(
                image.get_pixel(width / 2, height - 2).0[3],
                255,
                "the Settings body remains opaque between rounded corners"
            );
        }
    }
}
