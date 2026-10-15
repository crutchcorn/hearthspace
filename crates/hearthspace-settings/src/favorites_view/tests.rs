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

#[path = "test_support.rs"]
mod support;
use support::Ui;

fn state() -> (tempfile::TempDir, SettingsApp) {
    let (dir, favorites) = fixture();
    (
        dir,
        SettingsApp {
            favorites,
            settings: Settings::default(),
            live_outputs: vec![],
            selected_category: SettingsCategory::Favorites,
            selected_monitor_id: None,
            background_picker_request_sender: None,
            background_picker_pending: false,
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
fn accessible_add_assign_draw_and_save_updates_the_real_store_only_after_save() {
    let (dir, state) = state();
    let mut ui = Ui::new(state, view, (760, 700));
    let add = ui.button("Add Alpha to favorites");
    ui.activate(add);
    ui.dispatch(&view);
    let assign = ui.button("Assign gesture for Alpha");
    ui.activate(assign);
    ui.dispatch(&view);
    let path = dir.path().join("favorites.json");
    let original = std::fs::read(&path).unwrap();
    let save = ui.button("Save gesture");
    assert!(ui.harness.access_node(save).unwrap().is_disabled());
    draw(&mut ui, &[[30.0, 40.0], [90.0, 130.0], [150.0, 40.0]]);
    ui.dispatch(&view);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let save = ui.button("Save gesture");
    assert!(!ui.harness.access_node(save).unwrap().is_disabled());
    ui.activate(save);
    ui.dispatch(&view);
    assert!(ui.state.favorites.recording.is_none());
    assert!(ui.state.favorites.entries()[0].gesture.is_some());
    named(&ui, "Assigned gesture", Role::Image);
    ui.button("Change gesture for Alpha");
}

#[test]
fn named_search_filters_apps_and_the_recording_preview_can_be_redrawn() {
    let (_dir, state) = state();
    let mut ui = Ui::new(state, view, (760, 700));
    let search = named(&ui, "Find apps", Role::TextInput);
    ui.harness.focus_on(Some(search));
    ui.harness.keyboard_type_chars("Beta");
    ui.dispatch(&view);
    assert_eq!(ui.state.favorites.query, "Beta");
    let add = ui.button("Add Beta to favorites");
    ui.activate(add);
    ui.dispatch(&view);
    let assign = ui.button("Assign gesture for Beta");
    ui.activate(assign);
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
    let retry = ui.button("Try again");
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
    let save = ui.button("Save gesture");
    assert!(ui.harness.access_node(save).unwrap().is_disabled());
    let cancel = ui.button("Cancel recording");
    ui.activate(cancel);
    ui.dispatch(&view);
    assert!(ui.state.favorites.recording.is_none());
    assert!(ui.state.favorites.entries()[0].gesture.is_none());
}

#[test]
fn switching_settings_categories_cancels_recording_and_late_completion() {
    let (_dir, mut state) = state();
    state.favorites.add("Alpha.desktop");
    state.favorites.start("Alpha.desktop");
    let generation = state.favorites.recording.as_ref().unwrap().generation;
    state.select_category(SettingsCategory::Display);
    state.favorites.stroke(
        "Alpha.desktop",
        generation,
        crate::favorites::tests::shape(),
    );
    assert!(state.favorites.recording.is_none());
    assert!(state.favorites.entries()[0].gesture.is_none());
}
