use crate::{
    favorites::tests::fixture,
    state::{SettingsApp, SettingsCategory},
};
use hearthspace_ipc::Settings;
use masonry::{
    accesskit::Role,
    core::{WidgetId, pointer::PointerButton},
};
use xilem::{WidgetView, view::sized_box};

use crate::test_support as support;
use support::Ui;

pub(crate) fn state() -> (tempfile::TempDir, SettingsApp) {
    let (dir, favorites) = fixture();
    (
        dir,
        SettingsApp {
            activation_listener: None,
            ai: crate::ai::AiSettings::default(),
            favorites,
            settings: Settings::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Gestures,
            selected_monitor_id: None,
            background_picker_request_sender: None,
            background_picker_pending: false,
            background_fit_open: false,
            status: String::new(),
            config_label: String::new(),
        },
    )
}

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(super::view(state))
}

fn named<V: WidgetView<SettingsApp>>(ui: &Ui<V>, name: &str, role: Role) -> WidgetId
where
    V::Widget: Sized,
{
    support::ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .access_node(*id)
                .is_some_and(|node| node.label().as_deref() == Some(name) && node.role() == role)
        })
        .unwrap_or_else(|| panic!("missing {role:?}: {name}"))
}

fn draw<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>, points: &[[f64; 2]])
where
    V::Widget: Sized,
{
    let id = named(ui, "Gesture drawing area", Role::Canvas);
    let rect = ui.harness.get_widget_with_id(id).ctx().bounding_box();
    for (index, point) in points.iter().enumerate() {
        ui.harness
            .mouse_move((rect.x0 + point[0], rect.y0 + point[1]));
        if index == 0 {
            ui.harness.mouse_button_press(Some(PointerButton::Primary));
        }
    }
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
}

#[test]
fn accessible_add_gesture_draw_and_save_updates_the_real_store_only_after_save() {
    let (dir, state) = state();
    let mut ui = Ui::new(state, view, (760, 700));
    let add = ui.button("Add gesture");
    ui.activate(add);
    ui.dispatch(&view);
    let path = dir.path().join("favorites.json");
    assert!(!path.exists());
    let save = ui.button("Save shortcut");
    assert!(ui.harness.access_node(save).unwrap().is_disabled());
    let initial_pad = ui
        .harness
        .get_widget_with_id(named(&ui, "Gesture drawing area", Role::Canvas))
        .ctx()
        .bounding_box();
    draw(&mut ui, &[[30.0, 40.0], [90.0, 130.0], [150.0, 40.0]]);
    ui.dispatch(&view);
    assert!(!path.exists());
    assert_eq!(
        ui.harness
            .get_widget_with_id(named(&ui, "Gesture drawing area", Role::Canvas))
            .ctx()
            .bounding_box(),
        initial_pad
    );
    let save = ui.button("Save shortcut");
    assert!(!ui.harness.access_node(save).unwrap().is_disabled());
    ui.activate(save);
    ui.dispatch(&view);
    assert!(ui.state.favorites.recording.is_none());
    assert!(ui.state.favorites.entries()[0].gesture.is_some());
    named(&ui, "Assigned gesture", Role::Image);
    ui.button("Change gesture for Alpha");
}

#[test]
fn repeated_gestures_activation_preserves_an_unsaved_recording() {
    let (_dir, mut state) = state();
    state.favorites.begin_add();
    let generation = state.favorites.recording.as_ref().unwrap().generation;
    state.favorites.stroke(
        "Alpha.desktop",
        generation,
        crate::favorites::tests::shape(),
    );
    let points = state.favorites.recording.as_ref().unwrap().points.clone();
    crate::activation::activate(&mut state, SettingsCategory::Gestures);
    let recording = state.favorites.recording.as_ref().unwrap();
    assert_eq!(recording.generation, generation);
    assert_eq!(recording.points, points);
    assert!(recording.valid);
    assert!(state.favorites.entries().is_empty());
    crate::activation::activate(&mut state, SettingsCategory::Appearance);
    assert_eq!(state.selected_category, SettingsCategory::Appearance);
    assert!(state.favorites.recording.is_none());
}

#[test]
fn long_app_names_keep_list_actions_and_editor_controls_inside_narrow_content() {
    let (directory, mut state) = state();
    let long_name = "A very long installed application name with extra words and details";
    std::fs::write(
        directory.path().join("applications/Alpha.desktop"),
        format!("[Desktop Entry]\nType=Application\nName={long_name}\nExec=false\n"),
    )
    .unwrap();
    state.favorites.catalog =
        hearthspace_shell::app_catalog::AppCatalog::load_from_data_dirs(vec![
            directory.path().to_owned(),
        ]);
    state
        .favorites
        .store
        .as_mut()
        .unwrap()
        .set_gesture("Alpha.desktop", &crate::favorites::tests::shape())
        .unwrap();
    let mut ui = Ui::new(state, view, (300, 650));
    for name in [
        format!("Change gesture for {long_name}"),
        format!("Remove gesture for {long_name}"),
    ] {
        let bounds = ui
            .harness
            .get_widget_with_id(ui.button(&name))
            .ctx()
            .bounding_box();
        assert!(bounds.x0 >= 0.0 && bounds.x1 <= 300.0, "{name}: {bounds:?}");
        assert!(bounds.width() >= 44.0 && bounds.height() >= 44.0);
    }
    let edit = ui.button(&format!("Change gesture for {long_name}"));
    ui.activate(edit);
    ui.dispatch(&view);
    for name in [
        "Back to gestures".to_owned(),
        format!("{long_name} Choose app"),
        "Save shortcut".into(),
    ] {
        let bounds = ui
            .harness
            .get_widget_with_id(ui.button(&name))
            .ctx()
            .bounding_box();
        assert!(bounds.x0 >= 0.0 && bounds.x1 <= 300.0, "{name}: {bounds:?}");
    }
    draw(&mut ui, &[[30.0, 40.0], [90.0, 130.0], [150.0, 40.0]]);
    ui.dispatch(&view);
    let retry = ui.button("Retry");
    ui.activate(retry);
    ui.dispatch(&view);
    draw(&mut ui, &[[40.0, 40.0], [40.0, 140.0], [160.0, 140.0]]);
    ui.dispatch(&view);
    assert!(ui.state.favorites.recording.as_ref().unwrap().valid);
    assert!(
        !ui.harness
            .access_node(ui.button("Save shortcut"))
            .unwrap()
            .is_disabled()
    );
}

#[test]
fn named_search_filters_apps_and_the_recording_preview_can_be_redrawn() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (760, 700));
    let add = ui.button("Add gesture");
    ui.activate(add);
    ui.dispatch(&view);
    let picker = ui.button("Alpha Choose app");
    ui.activate(picker);
    ui.dispatch(&view);
    let search = named(&ui, "Find apps", Role::TextInput);
    ui.harness.focus_on(Some(search));
    ui.harness.keyboard_type_chars("Beta");
    ui.dispatch(&view);
    assert_eq!(ui.state.favorites.query, "Beta");
    let add = ui.button("Beta");
    ui.activate(add);
    ui.dispatch(&view);
    draw(&mut ui, &[[30.0, 40.0], [90.0, 130.0], [150.0, 40.0]]);
    ui.dispatch(&view);
    let first = ui
        .state
        .favorites
        .recording
        .as_ref()
        .unwrap()
        .points
        .clone();
    draw(&mut ui, &[[40.0, 40.0], [40.0, 140.0], [160.0, 140.0]]);
    ui.dispatch(&view);
    let record = ui.state.favorites.recording.as_ref().unwrap();
    assert!(record.valid);
    assert_ne!(record.points, first);
    let drawn = record.points.clone();
    let picker = ui.button("Beta Choose app");
    ui.activate(picker);
    ui.dispatch(&view);
    assert_eq!(
        ui.harness
            .access_node(ui.button("Beta Choose app"))
            .unwrap()
            .data()
            .is_expanded(),
        Some(true)
    );
    let choice = ui.button("Beta");
    ui.harness.focus_on(Some(choice));
    ui.harness
        .process_text_event(masonry::core::TextEvent::key_down(
            masonry::core::keyboard::Key::Named(masonry::core::keyboard::NamedKey::Enter),
        ));
    ui.harness
        .process_text_event(masonry::core::TextEvent::key_up(
            masonry::core::keyboard::Key::Named(masonry::core::keyboard::NamedKey::Enter),
        ));
    ui.dispatch(&view);
    assert_eq!(ui.state.favorites.recording.as_ref().unwrap().points, drawn);
    assert!(ui.state.favorites.recording.as_ref().unwrap().valid);
    let picker = ui.button("Beta Choose app");
    assert_eq!(
        ui.harness.access_node(picker).unwrap().data().is_expanded(),
        Some(false)
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(picker));
    let retry = ui.button("Retry");
    ui.activate(retry);
    ui.dispatch(&view);
    assert!(
        ui.state
            .favorites
            .recording
            .as_ref()
            .unwrap()
            .points
            .is_empty()
    );
    let save = ui.button("Save shortcut");
    assert!(ui.harness.access_node(save).unwrap().is_disabled());
    let cancel = ui.button("Back to gestures");
    ui.activate(cancel);
    ui.dispatch(&view);
    assert!(ui.state.favorites.recording.is_none());
    assert!(ui.state.favorites.entries().is_empty());
}

#[test]
fn switching_settings_categories_cancels_recording_and_late_completion() {
    let (_dir, mut state) = state();
    state.favorites.start("Alpha.desktop");
    let generation = state.favorites.recording.as_ref().unwrap().generation;
    state.select_category(SettingsCategory::Displays);
    state.favorites.stroke(
        "Alpha.desktop",
        generation,
        crate::favorites::tests::shape(),
    );
    assert!(state.favorites.recording.is_none());
    assert!(state.favorites.entries().is_empty());
}

#[test]
fn gestures_category_has_only_gesture_controls_and_hides_legacy_empty_entries() {
    let (_dir, mut state) = state();
    state
        .favorites
        .store
        .as_mut()
        .unwrap()
        .add("Alpha.desktop")
        .unwrap();
    assert_eq!(SettingsCategory::Gestures.label(), "Gestures");
    let ui = Ui::new(state, view, (760, 700));
    ui.button("Add gesture");
    let mut labels = Vec::new();
    for id in support::ids(&ui.harness) {
        if let Some(node) = ui.harness.access_node(id) {
            labels.extend(node.label());
            labels.extend(node.value());
        }
    }
    assert!(labels.iter().any(|label| label == "Add gesture"));
    assert!(
        !labels
            .iter()
            .any(|label| label.to_lowercase().contains("favorite"))
    );
    assert!(!labels.iter().any(|label| label == "Change gesture for Alpha" || label == "Remove gesture for Alpha"));
}

#[test]
fn remove_gesture_button_deletes_assignment_and_reoffers_add_gesture() {
    let (dir, mut state) = state();
    state
        .favorites
        .store
        .as_mut()
        .unwrap()
        .set_gesture("Alpha.desktop", &crate::favorites::tests::shape())
        .unwrap();
    let mut ui = Ui::new(state, view, (760, 700));
    let remove = ui.button("Remove gesture for Alpha");
    ui.activate(remove);
    ui.dispatch(&view);
    ui.button("Add gesture");
    assert!(ui.state.favorites.entries().is_empty());
    assert!(
        hearthspace_shell::favorites::FavoriteStore::load(dir.path().join("favorites.json"))
            .unwrap()
            .entries()
            .is_empty()
    );
}
