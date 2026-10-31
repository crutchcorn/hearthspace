use super::*;
use crate::ShellCommand;
use hearthspace_desktop::app_catalog::AppCatalog;
use masonry::core::WindowEvent;
use std::fs;

#[path = "launcher_layout_tests.rs"]
mod layout;

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
    use crate::ui::launch::tests::worker as launch_worker;
    let directory = tempfile::tempdir().unwrap();
    let mut state = installed_apps(14);
    state.command_socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = launch_worker::read_request(&mut stream);
        let command = ShellCommand::LaunchAppOnOutput {
            app_id: "app-13.desktop".into(),
            output_name: "eDP-1".into(),
        };
        assert_eq!(request, format!("{}\n", command.wire_name()));
        stream.write_all(b"ok\n").unwrap();
        launch_worker::reply_active(&listener);
    });
    let mut worker = launch_worker::launch(&mut state);
    let mut ui = Ui::new(state, view, (360, 360));
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
    assert!(selected_bounds.y0 >= 130.0 && selected_bounds.y1 <= 360.0);
    assert_eq!(ui.harness.focused_widget_id(), Some(input));
    assert!(ui.state.outputs[0].query.is_empty());
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(
        ui.state.outputs[0].show_search,
        "acceptance has not been dispatched yet"
    );
    worker.complete(&mut ui.state);
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].show_search);
    server.join().unwrap();
}

#[test]
fn typing_filters_apps_and_clearing_restores_browsing_after_resize() {
    let mut ui = Ui::new(installed_apps(14), view, (768, 560));
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
    let focused = ui.harness.focused_widget_id().unwrap();
    assert_eq!(
        ui.harness.access_node(focused).unwrap().role(),
        Role::TextInput
    );
}

#[test]
fn grid_and_list_switch_preserves_selection_and_exposes_the_current_layout() {
    use masonry::accesskit::Toggled;
    let mut ui = Ui::new(installed_apps(8), view, (650, 560));
    let bounds = |ui: &Ui<_>, name| {
        ui.harness
            .access_node(ui.button(name))
            .unwrap()
            .bounding_box()
            .unwrap()
    };
    let first = bounds(&ui, "Open: App 00");
    let fourth = bounds(&ui, "Open: App 03");
    let fifth = bounds(&ui, "Open: App 04");
    assert_eq!(first.y0, fourth.y0);
    assert!(fourth.x0 > first.x1);
    assert!(fifth.y0 > first.y1);
    assert_eq!(
        ui.harness
            .access_node(ui.button("Show app grid"))
            .unwrap()
            .toggled(),
        Some(Toggled::True)
    );
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
    ui.dispatch(&view);
    assert_eq!(selected(&ui), ["Open: App 04"]);

    ui.activate(ui.button("Show app list"));
    ui.dispatch(&view);
    assert_eq!(selected(&ui), ["Open: App 04"]);
    assert_eq!(
        ui.harness
            .access_node(ui.button("Show app list"))
            .unwrap()
            .toggled(),
        Some(Toggled::True)
    );
    assert_eq!(
        ui.harness
            .access_node(ui.button("Show app grid"))
            .unwrap()
            .toggled(),
        Some(Toggled::False)
    );
    let first = bounds(&ui, "Open: App 00");
    let second = bounds(&ui, "Open: App 01");
    assert_eq!(first.x0, second.x0);
    assert!(second.y0 >= first.y1);
    ui.state.change_query("eDP-1", "App 07".into());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(selected(&ui), ["Open: App 07"]);
    ui.state.change_query("eDP-1", String::new());
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(
        ui.state.outputs[0].launcher_layout,
        launcher::AppLayout::List
    );
}

#[test]
fn narrow_grids_keep_tiles_readable_and_arrow_navigation_follows_visible_rows() {
    for (width, columns) in [(272, 2), (360, 3), (650, 4)] {
        let mut ui = Ui::new(installed_apps(8), view, (width, 560));
        let bounds = |ui: &Ui<_>, index| {
            ui.harness
                .access_node(ui.button(&format!("Open: App {index:02}")))
                .unwrap()
                .bounding_box()
                .unwrap()
        };
        let first = bounds(&ui, 0);
        let last_in_row = bounds(&ui, columns - 1);
        let next_row = bounds(&ui, columns);
        assert_eq!(first.y0, last_in_row.y0);
        assert!(next_row.y0 > first.y1);
        for index in 0..8 {
            let tile = bounds(&ui, index);
            assert!(tile.width() >= 100.0, "{width}: {tile:?}");
            assert!(tile.x0 >= 0.0 && tile.x1 <= f64::from(width), "{tile:?}");
        }
        let input = ui.harness.focused_widget_id().unwrap();
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        ui.dispatch(&view);
        assert_eq!(selected(&ui), [format!("Open: App {columns:02}")]);
        assert_eq!(ui.harness.focused_widget_id(), Some(input));

        ui.harness.focus_on(Some(ui.button("Open: App 01")));
        ui.dispatch(&view);
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        ui.dispatch(&view);
        assert_eq!(selected(&ui), [format!("Open: App {:02}", columns + 1)]);
        assert_eq!(ui.harness.focused_widget_id(), Some(input));
    }
}

#[test]
fn rebuilding_launcher_controls_preserves_tab_focus() {
    let mut ui = Ui::new(installed_apps(8), view, (650, 560));
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Tab)));
    ui.dispatch(&view);
    let tab_target = ui.harness.focused_widget_id().unwrap();
    assert_eq!(
        ui.harness.access_node(tab_target).unwrap().role(),
        Role::Button
    );
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(tab_target));
}

#[test]
fn drawing_tab_returns_focus_to_search_and_escape_dismisses_without_an_editor() {
    use masonry::accesskit::Toggled;
    let mut ui = Ui::new(installed_apps(2), view, (650, 560));
    let draw = ui.button("Draw to launch");
    ui.harness.focus_on(Some(draw));
    // A release inherited from another control cannot activate this tab.
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].drawing.active());
    assert_eq!(ui.harness.focused_widget_id(), Some(draw));
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    let TextEvent::Keyboard(mut repeat) = TextEvent::key_down(Key::Named(NamedKey::Enter)) else {
        unreachable!();
    };
    repeat.repeat = true;
    ui.harness.process_text_event(TextEvent::Keyboard(repeat));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].drawing.active());
    assert_eq!(ui.harness.focused_widget_id(), Some(draw));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].drawing.active());
    assert_eq!(
        ui.harness
            .access_node(ui.button("Draw to launch"))
            .unwrap()
            .toggled(),
        Some(Toggled::True)
    );
    ui.activate(ui.button("Back to apps list"));
    ui.dispatch(&view);
    let input = ui.harness.focused_widget_id().unwrap();
    assert_eq!(
        ui.harness.access_node(input).unwrap().role(),
        Role::TextInput
    );
    ui.harness.mouse_click_on(ui.button("Draw to launch"), None);
    ui.dispatch(&view);
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Escape)));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].show_search);
    assert!(!ui.state.outputs[0].drawing.active());
}
