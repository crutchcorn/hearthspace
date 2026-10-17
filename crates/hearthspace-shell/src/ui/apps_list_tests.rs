use super::*;
use crate::{ShellCommand, app_catalog::AppCatalog};
use masonry::core::WindowEvent;
use std::fs;

fn installed_apps(count: usize) -> ShellState {
    let directory = tempfile::tempdir().unwrap();
    let applications = directory.path().join("applications");
    fs::create_dir(&applications).unwrap();
    for index in (0..count).rev() {
        fs::write(
            applications.join(format!("app-{index:02}.desktop")),
            format!("[Desktop Entry]\nType=Application\nName=App {index:02}\nExec=true\n"),
        )
        .unwrap();
    }
    let mut state = state();
    state.catalog = AppCatalog::load_from_data_dirs(vec![directory.path().into()]);
    state.change_query("eDP-1", String::new());
    reconcile(&mut state);
    state
}

#[test]
fn opening_apps_list_browses_every_installed_app_in_name_order() {
    let state = installed_apps(crate::ui::constants::MAX_RESULTS + 3);
    let entries = launcher::results(&state, &state.outputs[0]);
    assert_eq!(entries.len(), state.catalog.apps().len());
    assert!(
        entries
            .iter()
            .all(|entry| matches!(entry, launcher::Entry::App(_)))
    );
    let names = entries
        .iter()
        .map(|entry| entry.label())
        .collect::<Vec<_>>();
    let expected = (0..entries.len())
        .map(|index| format!("Open: App {index:02}"))
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
}

#[test]
fn blank_catalog_explains_empty_state_and_has_no_selected_result() {
    let mut ui = Ui::new(installed_apps(0), view, (360, 176));
    assert!(ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.value().as_deref() == Some("No installed apps found")
                || node.label().as_deref() == Some("No installed apps found")
        })
    }));
    assert!(selected(&ui).is_empty());
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
    ui.dispatch(&view);
    assert!(selected(&ui).is_empty());
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].show_search);
}

#[test]
fn browsing_scrolls_through_all_apps_and_enter_launches_on_the_opened_output() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = installed_apps(14);
    state.command_socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        stream.read_to_string(&mut request).unwrap();
        let command = ShellCommand::LaunchAppOnOutput {
            app_id: "app-13.desktop".into(),
            output_name: "eDP-1".into(),
        };
        assert_eq!(request, format!("{}\n", command.wire_name()));
        stream.write_all(b"ok\n").unwrap();
    });
    let mut ui = Ui::new(state, view, (360, 220));
    let input = ui.harness.focused_widget_id().unwrap();
    assert_eq!(selected(&ui), ["Open: App 00"]);
    for _ in 0..13 {
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        ui.dispatch(&view);
    }
    assert_eq!(selected(&ui), ["Open: App 13"]);
    let selected_bounds = ui
        .harness
        .access_node(ui.button("Open: App 13"))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(selected_bounds.y0 >= 90.0 && selected_bounds.y1 <= 208.0);
    assert_eq!(ui.harness.focused_widget_id(), Some(input));
    assert!(ui.state.outputs[0].query.is_empty());
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].show_search);
    server.join().unwrap();
}

#[test]
fn typing_filters_apps_and_clearing_restores_browsing_after_resize() {
    let mut ui = Ui::new(installed_apps(14), view, (768, 560));
    let input = ui.harness.focused_widget_id().unwrap();
    ui.harness
        .process_text_event(TextEvent::ClipboardPaste("App 13".into()));
    ui.dispatch(&view);
    assert_eq!(selected(&ui), ["Open: App 13"]);
    let entries = launcher::results(&ui.state, &ui.state.outputs[0]);
    assert_eq!(entries.len(), 1);
    ui.harness
        .process_window_event(WindowEvent::Resize((280, 176).into()));
    ui.dispatch(&view);
    let close = ui
        .harness
        .access_node(ui.button("Close apps list"))
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(close.x0 >= 0.0 && close.x1 <= 280.0);
    assert!(close.y0 >= 0.0 && close.y1 <= 176.0);
    ui.state.change_query("eDP-1", String::new());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(launcher::results(&ui.state, &ui.state.outputs[0]).len(), 14);
    assert_eq!(selected(&ui), ["Open: App 00"]);
    assert_eq!(ui.harness.focused_widget_id(), Some(input));
}
