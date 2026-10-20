use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    process::Command,
};

use crate::{
    state::{SettingsApp, SettingsCategory},
    test_support::{Ui, ids},
};
use hearthspace_ipc::{BackgroundMode, Settings, SettingsRequest};
use xilem::{
    WidgetView,
    masonry::{
        accesskit::Role,
        core::{
            PointerButton, TextEvent, WidgetId, WindowEvent,
            keyboard::{Key, NamedKey},
        },
        kurbo::Rect,
        layout::AsUnit,
        properties::Padding,
    },
    style::Style,
    view::{label, sized_box},
};

fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    sized_box(if state.selected_category == SettingsCategory::Background {
        super::view(state)
    } else {
        label("Another Settings page").boxed()
    })
}

fn lower_view(state: &SettingsApp) -> impl WidgetView<SettingsApp, Widget: Sized> + use<> {
    view(state).padding(Padding::top(120.0.px()))
}

fn combobox<V: WidgetView<SettingsApp>>(ui: &Ui<V>) -> WidgetId
where
    V::Widget: Sized,
{
    ids(&ui.harness)
        .into_iter()
        .find(|&id| {
            ui.harness
                .access_node(id)
                .is_some_and(|node| node.role() == Role::ComboBox)
        })
        .expect("Photo fit combobox")
}

fn layer_nodes<V: WidgetView<SettingsApp>>(ui: &Ui<V>, role: Role) -> Vec<(String, Rect, bool)>
where
    V::Widget: Sized,
{
    // Layers are siblings of the base view, so inspect the complete access tree.
    let mut pending = vec![ui.harness.access_tree().state().root()];
    let mut result = Vec::new();
    while let Some(node) = pending.pop() {
        pending.extend(node.children());
        if node.role() == role {
            let bounds = node.bounding_box().expect("popup node bounds");
            result.push((
                node.label().unwrap_or_default(),
                Rect::new(bounds.x0, bounds.y0, bounds.x1, bounds.y1),
                node.is_selected() == Some(true),
            ));
        }
    }
    result
}

fn key<V: WidgetView<SettingsApp>>(ui: &mut Ui<V>, key: NamedKey)
where
    V::Widget: Sized,
{
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(key)));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(key)));
    ui.harness.redraw();
}

#[test]
fn photo_fit_selection_and_reset_keep_keyboard_focus() {
    const CHILD: &str = "HEARTHSPACE_TEST_BACKGROUND_FOCUS";
    // Isolate the real IPC client from the user's settings daemon without
    // mutating environment variables alongside parallel tests.
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir().unwrap();
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "background_view::tests::photo_fit_selection_and_reset_keep_keyboard_focus",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env(
                hearthspace_common::SETTINGS_SOCKET_ENV,
                directory.path().join("settings.sock"),
            )
            .env(
                hearthspace_common::SHELL_COMMAND_SOCKET_ENV,
                directory.path().join("no-compositor.sock"),
            )
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let listener = UnixListener::bind(hearthspace_ipc::settings_socket_path().unwrap()).unwrap();
    let settings = Settings {
        background_image_path: Some("photo.png".into()),
        background_mode: BackgroundMode::RepeatWithCanvas,
        ..Settings::default()
    };
    let server_settings = settings.clone();
    let backend = std::thread::spawn(move || {
        let mut settings = server_settings;
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = String::new();
            BufReader::new(&stream).read_line(&mut request).unwrap();
            match hearthspace_ipc::parse_settings_request(&request).unwrap() {
                SettingsRequest::SetBackgroundMode(mode) => settings.background_mode = mode,
                SettingsRequest::SetBackgroundImagePath(path) => {
                    settings.background_image_path = path
                }
                request => panic!("unexpected request: {request:?}"),
            }
            stream
                .write_all(hearthspace_ipc::settings_response(settings.clone()).as_bytes())
                .unwrap();
        }
        settings
    });
    let (_directory, favorites) = crate::favorites::tests::fixture();
    let state = SettingsApp {
        activation_listener: None,
        ai: Default::default(),
        favorites,
        settings,
        live_outputs: vec![],
        selected_category: SettingsCategory::Background,
        selected_monitor_id: None,
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    let mut ui = Ui::new(state, view, (440, 420));
    let upload = ui.button("Change photo");
    assert_ne!(
        ui.harness.focused_widget_id(),
        Some(upload),
        "no initial autofocus"
    );
    let opener = combobox(&ui);
    let reset = ui.button("Use default background");
    let before = ui.harness.get_widget_with_id(reset).ctx().bounding_box();
    ui.harness.focus_on(Some(opener));
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    assert_eq!(layer_nodes(&ui, Role::ListBox).len(), 1);
    let choices = layer_nodes(&ui, Role::ListBoxOption);
    assert_eq!(choices.len(), 4);
    assert!(
        choices
            .iter()
            .any(|(name, _, selected)| name == "Repeat with canvas" && *selected)
    );
    assert_eq!(
        ui.harness.get_widget_with_id(reset).ctx().bounding_box(),
        before,
        "opening the dropdown must not expand the settings page"
    );
    for (key_value, expected) in [
        (NamedKey::ArrowDown, "Stretch"),
        (NamedKey::End, "Cover"),
        (NamedKey::Home, "Repeat with canvas"),
        (NamedKey::ArrowUp, "Repeat with canvas"),
        (NamedKey::End, "Cover"),
    ] {
        key(&mut ui, key_value);
        assert_eq!(
            ui.harness
                .access_node(opener)
                .unwrap()
                .active_descendant()
                .unwrap()
                .label()
                .as_deref(),
            Some(expected)
        );
    }
    key(&mut ui, NamedKey::Enter);
    ui.dispatch(&view);
    assert_eq!(ui.state.settings.background_mode, BackgroundMode::Cover);
    assert!(layer_nodes(&ui, Role::ListBox).is_empty());
    assert_eq!(ui.harness.focused_widget_id(), Some(opener));
    assert_eq!(
        ui.harness.access_node(opener).unwrap().value().as_deref(),
        Some("Cover")
    );
    key(&mut ui, NamedKey::Enter);
    let contain = layer_nodes(&ui, Role::ListBoxOption)
        .into_iter()
        .find(|(name, _, _)| name == "Contain")
        .unwrap()
        .1
        .center();
    ui.harness.mouse_move(contain);
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert_eq!(ui.state.settings.background_mode, BackgroundMode::Contains);
    assert_eq!(ui.harness.focused_widget_id(), Some(opener));
    assert!(layer_nodes(&ui, Role::ListBox).is_empty());
    key(&mut ui, NamedKey::Enter);
    key(&mut ui, NamedKey::Escape);
    ui.dispatch(&view);
    assert!(layer_nodes(&ui, Role::ListBox).is_empty());
    assert_eq!(ui.harness.focused_widget_id(), Some(opener));
    key(&mut ui, NamedKey::Enter);
    ui.harness.mouse_move((2.0, 2.0));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&view);
    assert!(layer_nodes(&ui, Role::ListBox).is_empty());
    assert_eq!(ui.harness.focused_widget_id(), Some(opener));
    ui.harness
        .focus_on(Some(ui.button("Use default background")));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert_eq!(
        ui.button("Upload photo"),
        upload,
        "picker identity stays stable"
    );
    assert_eq!(ui.harness.focused_widget_id(), Some(upload));
    assert!(ui.state.settings.background_image_path.is_none());
    let saved = backend.join().unwrap();
    assert_eq!(saved.background_mode, BackgroundMode::Contains);
    assert!(saved.background_image_path.is_none());
}

#[test]
fn popup_flips_and_clamps_to_the_window_and_cleans_up_when_disabled_or_page_changes() {
    let (_directory, favorites) = crate::favorites::tests::fixture();
    let state = SettingsApp {
        activation_listener: None,
        ai: Default::default(),
        favorites,
        settings: Settings {
            background_image_path: Some("photo.png".into()),
            ..Settings::default()
        },
        live_outputs: vec![],
        selected_category: SettingsCategory::Background,
        selected_monitor_id: None,
        background_picker_request_sender: None,
        background_picker_pending: false,
        status: String::new(),
    };
    let mut ui = Ui::new(state, lower_view, (320, 320));
    let opener = combobox(&ui);
    let anchor = ui.harness.get_widget_with_id(opener).ctx().bounding_box();
    ui.harness.focus_on(Some(opener));
    key(&mut ui, NamedKey::Enter);
    let menu = layer_nodes(&ui, Role::ListBox)[0].1;
    assert!(menu.x0 >= 8.0 && menu.y0 >= 8.0 && menu.x1 <= 312.0 && menu.y1 <= 312.0);
    assert!(
        menu.y1 <= anchor.y0,
        "the popup must flip above the lower opener: {menu:?}, {anchor:?}"
    );
    ui.harness
        .process_window_event(WindowEvent::Resize((460, 320).into()));
    ui.harness.redraw();
    let moved_anchor = ui.harness.get_widget_with_id(opener).ctx().bounding_box();
    assert!(
        moved_anchor.x1 > anchor.x1 + 100.0,
        "the wider window must move the opener"
    );
    let menu = layer_nodes(&ui, Role::ListBox)[0].1;
    assert!(
        (menu.x1 - moved_anchor.x1.min(452.0)).abs() < 0.01,
        "the open popup must follow the relocated opener: {menu:?}, {moved_anchor:?}"
    );
    assert!(menu.y1 <= moved_anchor.y0);
    ui.harness
        .process_window_event(WindowEvent::Resize((240, 220).into()));
    ui.harness.redraw();
    let menu = layer_nodes(&ui, Role::ListBox)[0].1;
    assert!(menu.x0 >= 8.0 && menu.y0 >= 8.0 && menu.x1 <= 232.0 && menu.y1 <= 212.0);
    ui.state.background_picker_pending = true;
    ui.rebuild(&lower_view);
    ui.dispatch(&lower_view);
    assert!(layer_nodes(&ui, Role::ListBox).is_empty());
    ui.activate(opener);
    ui.dispatch(&lower_view);
    assert!(layer_nodes(&ui, Role::ListBox).is_empty());
    ui.state.background_picker_pending = false;
    ui.rebuild(&lower_view);
    ui.harness.focus_on(Some(opener));
    key(&mut ui, NamedKey::Enter);
    assert_eq!(layer_nodes(&ui, Role::ListBox).len(), 1);
    ui.state.selected_category = SettingsCategory::Appearance;
    ui.rebuild(&lower_view);
    ui.dispatch(&lower_view);
    assert!(
        layer_nodes(&ui, Role::ListBox).is_empty(),
        "page teardown must remove its popup"
    );
}
