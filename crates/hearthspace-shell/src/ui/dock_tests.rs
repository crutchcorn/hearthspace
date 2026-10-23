use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    thread,
};

use masonry::{
    accesskit::Role,
    core::{
        TextEvent,
        keyboard::{Key as KeyboardKey, NamedKey},
    },
};
use xilem::{WidgetView, view::sized_box};

use super::*;
use crate::{
    app_catalog::AppCatalog,
    ui::{
        test_support::{Ui, ids},
        window_catalog::RunningWindow,
    },
};

#[path = "dock_pinned_tests.rs"]
mod pinned;

fn state() -> ShellState {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.catalog = Default::default();
    state
}

fn window(id: u64, app_id: &str, focused: bool) -> RunningWindow {
    RunningWindow {
        id,
        app_id: app_id.into(),
        title: format!("Document {id}"),
        focused,
    }
}

fn strip(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(view(state, &state.outputs[0], 72.0))
}

#[test]
fn windows_group_by_app_and_keep_order_across_focus_and_title_changes() {
    let mut state = state();
    state.windows = vec![
        window(9, "editor", true),
        window(3, "browser", false),
        window(4, "editor", false),
        window(12, "", false),
        window(11, "", false),
    ];
    let groups = apps(&state);
    assert_eq!(
        groups.len(),
        4,
        "unknown applications must stay independent"
    );
    assert_eq!(groups[0].name, "browser");
    assert_eq!(
        groups[1]
            .windows
            .iter()
            .map(|window| window.id)
            .collect::<Vec<_>>(),
        [4, 9]
    );
    assert_eq!(groups[1].next_window().unwrap().id, 4);
    assert!(groups[1].label().contains("2 windows, active"));
    let before = groups.into_iter().map(|app| app.key).collect::<Vec<_>>();
    state.windows.reverse();
    state.windows[0].title = "Renamed document".into();
    for window in &mut state.windows {
        window.focused = window.id == 3;
    }
    assert_eq!(
        apps(&state)
            .into_iter()
            .map(|app| app.key)
            .collect::<Vec<_>>(),
        before
    );
}

#[test]
fn desktop_entry_supplies_human_name_and_icon_without_launching() {
    let dir = tempfile::tempdir().unwrap();
    let applications = dir.path().join("applications");
    std::fs::create_dir_all(&applications).unwrap();
    std::fs::write(
        applications.join("org.example.Editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Text Editor\nExec=unused\nIcon=editor-logo\n",
    )
    .unwrap();
    let mut state = state();
    state.catalog = AppCatalog::load_from_data_dirs(vec![dir.path().to_owned()]);
    state.windows = vec![window(4, "org.example.Editor", false)];
    let app = apps(&state).remove(0);
    assert_eq!(app.name, "Text Editor");
    assert_eq!(app.icon.as_deref(), Some("editor-logo"));
    assert_eq!(icons::requests(&state), ["editor-logo"]);
}

#[test]
fn shell_keyboard_focus_does_not_reset_window_cycling() {
    let mut state = state();
    state.windows = vec![window(2, "editor", false), window(7, "editor", true)];
    reconcile(&mut state);
    state.windows[1].focused = false;
    reconcile(&mut state);
    let app = apps(&state).remove(0);
    assert_eq!(app.next_window().unwrap().id, 2);
    assert!(app.focused());
    state.windows.retain(|window| window.id != 7);
    reconcile(&mut state);
    assert_eq!(state.dock.last_focused_window, None);
    assert_eq!(apps(&state)[0].next_window().unwrap().id, 2);
}

#[test]
fn pointer_and_keyboard_cycle_existing_windows_using_their_ids() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state();
    state.windows = vec![window(2, "editor", true), window(7, "editor", false)];
    state.command_socket = dir.path().join("command.sock");
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        for expected in ["focus-window 7\n", "focus-window 2\n"] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut command = String::new();
            stream.read_to_string(&mut command).unwrap();
            assert_eq!(command, expected);
            stream.write_all(b"ok\n").unwrap();
        }
    });
    let mut ui = Ui::new(state, strip, (72, 48));
    let button = ui.button("editor: 2 windows, active. Next: Document 7");
    ui.harness.mouse_click_on(button, None);
    ui.dispatch(&strip);
    assert!(ui.state.windows[1].focused);
    let button = ui.button("editor: 2 windows, active. Next: Document 2");
    ui.harness.focus_on(Some(button));
    ui.harness
        .process_text_event(TextEvent::key_up(KeyboardKey::Named(NamedKey::Enter)));
    ui.dispatch(&strip);
    assert!(ui.state.windows[0].focused);
    assert!(!ui.state.outputs[0].show_search);
    server.join().unwrap();
}

#[test]
fn narrow_dock_pages_every_app_and_recovers_after_last_page_apps_close() {
    let mut state = state();
    state.windows = (1..=5)
        .map(|id| window(id, &format!("app{id}"), false))
        .collect();
    let mut ui = Ui::new(state, strip, (72, 48));
    for page in 0..5 {
        ui.button(&format!("Switch to app{}", page + 1));
        for id in ids(&ui.harness) {
            if let Some(node) = ui.harness.access_node(id)
                && node.role() == Role::Button
            {
                let bounds = node.bounding_box().unwrap();
                assert!(bounds.x0 >= 0.0 && bounds.x1 <= 72.0, "{bounds:?}");
            }
        }
        let next = ui.button(&format!("Next apps (page {} of 5)", page + 1));
        ui.harness.mouse_click_on(next, None);
        ui.dispatch(&strip);
    }
    assert_eq!(ui.state.outputs[0].dock_page, 0);
    ui.state.outputs[0].dock_page = 4;
    ui.state.windows.truncate(2);
    ui.rebuild(&strip);
    ui.harness.redraw();
    ui.button("Switch to app2");
    ui.activate(ui.button("Next apps (page 2 of 2)"));
    ui.dispatch(&strip);
    ui.button("Switch to app1");
}

#[test]
fn closed_window_focus_failure_opens_actionable_apps_list_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state();
    state.windows = vec![window(9, "editor", false)];
    state.command_socket = dir.path().join("command.sock");
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut command = String::new();
        stream.read_to_string(&mut command).unwrap();
        assert_eq!(command, "focus-window 9\n");
        stream.write_all(b"err unknown window ID 9\n").unwrap();
    });
    let mut ui = Ui::new(state, strip, (72, 48));
    ui.activate(ui.button("Switch to editor"));
    ui.dispatch(&strip);
    assert!(ui.state.outputs[0].show_search);
    let error = ui.state.outputs[0].launch_error.as_deref().unwrap();
    assert!(error.contains("unknown window ID 9"));
    assert!(error.contains("Apps List to reopen"));
    server.join().unwrap();
}

#[test]
fn activating_removed_group_never_sends_a_launch_request() {
    let mut state = state();
    activate(&mut state, "eDP-1", &Key::App("closed".into()));
    assert!(state.outputs[0].show_search);
    assert!(
        state.outputs[0]
            .launch_error
            .as_deref()
            .unwrap()
            .contains("This app has closed")
    );
    assert_eq!(desired_width(&state), 0.0);
}

#[test]
fn held_pointer_does_not_activate_another_app_when_the_pressed_app_closes() {
    let mut state = state();
    state.windows = vec![window(1, "app1", false), window(2, "app2", false)];
    let mut ui = Ui::new(state, strip, (72, 48));
    ui.harness.mouse_move_to(ui.button("Switch to app1"));
    ui.harness
        .mouse_button_press(Some(masonry::core::PointerButton::Primary));
    ui.dispatch(&strip);
    ui.state.windows.remove(0);
    ui.rebuild(&strip);
    ui.harness
        .mouse_button_release(Some(masonry::core::PointerButton::Primary));
    ui.dispatch(&strip);
    assert!(!ui.state.outputs[0].show_search);
    assert!(ui.state.outputs[0].launch_error.is_none());
}

#[test]
fn sixty_eight_pixel_strip_keeps_compact_pager_and_app_inside_allocation() {
    let build = |state: &ShellState| sized_box(view(state, &state.outputs[0], 68.0));
    let mut state = state();
    state.windows = (1..=3)
        .map(|id| window(id, &format!("app{id}"), false))
        .collect();
    let mut ui = Ui::new(state, &build, (68, 54));
    for page in 1..=3 {
        let app = ui.button(&format!("Switch to app{page}"));
        let next = ui.button(&format!("Next apps (page {page} of 3)"));
        for button in [app, next] {
            let bounds = ui
                .harness
                .access_node(button)
                .unwrap()
                .bounding_box()
                .unwrap();
            assert!(bounds.x0 >= 0.0 && bounds.x1 <= 68.0, "{bounds:?}");
            assert!(bounds.width() >= 20.0);
        }
        ui.harness.mouse_click_on(next, None);
        ui.dispatch(&build);
    }
    assert_eq!(ui.state.outputs[0].dock_page, 0);
}

#[test]
fn single_control_fallback_cycles_all_apps_with_keyboard_focus_retained() {
    let build = |state: &ShellState| sized_box(view(state, &state.outputs[0], 40.0));
    let dir = tempfile::tempdir().unwrap();
    let mut state = state();
    state.windows = (1..=3)
        .map(|id| window(id, &format!("app{id}"), false))
        .collect();
    state.command_socket = dir.path().join("command.sock");
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        for expected in ["focus-window 1\n", "focus-window 2\n", "focus-window 3\n"] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut command = String::new();
            stream.read_to_string(&mut command).unwrap();
            assert_eq!(command, expected);
            stream.write_all(b"ok\n").unwrap();
        }
    });
    let mut ui = Ui::new(state, &build, (40, 48));
    let button = ui.button("Next app: Switch to app1");
    let bounds = ui
        .harness
        .access_node(button)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(bounds.x0 >= 0.0 && bounds.x1 <= 40.0, "{bounds:?}");
    ui.harness.focus_on(Some(button));
    for id in 1..=3 {
        assert_eq!(ui.button(&format!("Next app: Switch to app{id}")), button);
        ui.harness
            .process_text_event(TextEvent::key_up(KeyboardKey::Named(NamedKey::Enter)));
        ui.dispatch(&build);
        assert_eq!(ui.harness.focused_widget_id(), Some(button));
        assert!(
            ui.state
                .windows
                .iter()
                .any(|window| window.id == id && window.focused)
        );
    }
    ui.button("Next app: Switch to app1");
    server.join().unwrap();
}
