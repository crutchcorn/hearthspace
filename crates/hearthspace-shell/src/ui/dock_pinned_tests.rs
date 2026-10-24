use super::*;
use crate::ShellCommand;
use crate::ui::launch::tests::worker as launch_worker;

fn fixture(directory: &std::path::Path) -> ShellState {
    let applications = directory.join("applications");
    std::fs::create_dir_all(&applications).unwrap();
    for (id, name, metadata) in [
        (
            "editor",
            "Editor",
            "StartupWMClass=EditorClass\nIcon=editor-icon\n",
        ),
        (
            "steam_steam",
            "Steam",
            "X-SnapAppName=steam\nIcon=steam-icon\n",
        ),
    ] {
        std::fs::write(
            applications.join(format!("{id}.desktop")),
            format!("[Desktop Entry]\nType=Application\nName={name}\nExec=unused\n{metadata}"),
        )
        .unwrap();
    }
    let mut state = ShellState::new(
        directory.join("command.sock"),
        vec!["eDP-1".into(), "HDMI-A-1".into()],
    );
    state.catalog = AppCatalog::load_from_data_dirs(vec![directory.to_owned()]);
    state
}

#[test]
fn pins_merge_catalog_aliases_keep_saved_order_and_survive_last_window_closing() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = fixture(dir.path());
    for id in [
        "steam_steam.desktop",
        "editor.desktop",
        "removed.desktop",
        "steam_steam.desktop",
    ] {
        state.favorites.store.add(id).unwrap();
    }
    state.windows = vec![
        window(8, "uninstalled", false),
        window(5, "EditorClass", false),
        window(3, "editor.desktop", true),
        window(6, "Steam", false),
    ];
    let groups = apps(&state);
    assert_eq!(
        groups
            .iter()
            .map(|app| app.name.as_str())
            .collect::<Vec<_>>(),
        ["Steam", "Editor", "uninstalled"]
    );
    assert_eq!(groups[0].key, Key::App("steam_steam.desktop".into()));
    assert_eq!(
        groups[1]
            .windows
            .iter()
            .map(|window| window.id)
            .collect::<Vec<_>>(),
        [3, 5]
    );
    assert_eq!(groups[1].next_window().unwrap().id, 5);

    state.windows.retain(|window| window.id != 6);
    let closed = apps(&state);
    assert_eq!(closed[0].key, groups[0].key);
    assert_eq!(closed[0].label(), "Open Steam, pinned");
    assert!(closed[0].next_window().is_none());

    state.favorites.store.remove("editor.desktop").unwrap();
    let unpinned = apps(&state);
    assert_eq!(unpinned.len(), 3);
    assert!(!unpinned[1].pinned);
    assert_eq!(unpinned[1].windows.len(), 2);
    state.favorites.store.remove("steam_steam.desktop").unwrap();
    assert_eq!(apps(&state).len(), 2);
}

#[test]
fn closed_pin_launches_on_its_output_then_pointer_and_keyboard_cycle_existing_windows() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = fixture(dir.path());
    state.favorites.store.add("editor.desktop").unwrap();
    let build = |state: &ShellState| sized_box(view(state, &state.outputs[1], 188.0));
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let launch = ShellCommand::LaunchAppOnOutput {
            app_id: "editor.desktop".into(),
            output_name: "HDMI-A-1".into(),
        };
        for expected in [
            format!("{}\n", launch.wire_name()),
            "focus-window 2\n".into(),
            "focus-window 7\n".into(),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let command = launch_worker::read_request(&mut stream);
            assert_eq!(command, expected);
            stream.write_all(b"ok\n").unwrap();
            if expected.starts_with("launch-app") {
                launch_worker::reply_active(&listener);
            }
        }
    });
    let mut worker = launch_worker::launch(&mut state);
    let mut ui = Ui::new(state, &build, (188, 48));
    assert_eq!(desired_width(&ui.state), 44.0);
    let pin = ui.button("Open Editor, pinned");
    ui.harness.mouse_click_on(pin, None);
    ui.dispatch(&build);
    worker.complete(&mut ui.state);
    ui.state.windows = vec![window(7, "EditorClass", false), window(2, "editor", false)];
    ui.rebuild(&build);
    ui.dispatch(&build);
    assert_eq!(apps(&ui.state).len(), 1);
    let running = ui.button("Editor: 2 windows, pinned. Next: Document 2");
    assert_eq!(
        running, pin,
        "pin keeps its keyboard identity when the app opens"
    );
    ui.harness.focus_on(Some(running));
    ui.harness
        .process_text_event(TextEvent::key_up(KeyboardKey::Named(NamedKey::Enter)));
    ui.dispatch(&build);
    assert!(ui.state.windows[1].focused);
    ui.harness.mouse_click_on(
        ui.button("Editor: 2 windows, pinned, active. Next: Document 7"),
        None,
    );
    ui.dispatch(&build);
    assert!(ui.state.windows[0].focused);
    assert!(!ui.state.outputs[1].show_search);
    server.join().unwrap();
}

#[test]
fn rejected_pin_launch_retains_the_pin_and_exposes_the_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = fixture(dir.path());
    state.favorites.store.add("editor.desktop").unwrap();
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let command = launch_worker::read_request(&mut stream);
        assert_eq!(
            command,
            format!(
                "{}\n",
                ShellCommand::LaunchAppOnOutput {
                    app_id: "editor.desktop".into(),
                    output_name: "eDP-1".into()
                }
                .wire_name()
            )
        );
        stream.write_all(b"err missing executable\n").unwrap();
        launch_worker::reply_active(&listener);
    });
    let mut worker = launch_worker::launch(&mut state);
    let mut ui = Ui::new(state, strip, (72, 48));
    ui.activate(ui.button("Open Editor, pinned"));
    ui.dispatch(&strip);
    worker.complete(&mut ui.state);
    ui.rebuild(&strip);
    ui.dispatch(&strip);
    assert!(ui.state.outputs[0].show_search);
    assert!(
        ui.state.outputs[0]
            .launch_error
            .as_deref()
            .unwrap()
            .contains("missing executable")
    );
    ui.button("Open Editor, pinned");
    assert_eq!(ui.state.favorites.store.entries().len(), 1);
    server.join().unwrap();
}

#[test]
fn compact_control_reaches_every_closed_pin_and_running_app_without_losing_focus() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = fixture(dir.path());
    state.favorites.store.add("editor.desktop").unwrap();
    state.favorites.store.add("steam_steam.desktop").unwrap();
    state.windows = vec![window(9, "uninstalled", false)];
    let build = |state: &ShellState| sized_box(view(state, &state.outputs[0], 40.0));
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = thread::spawn(move || {
        for command in [
            ShellCommand::LaunchAppOnOutput {
                app_id: "editor.desktop".into(),
                output_name: "eDP-1".into(),
            }
            .wire_name(),
            ShellCommand::LaunchAppOnOutput {
                app_id: "steam_steam.desktop".into(),
                output_name: "eDP-1".into(),
            }
            .wire_name(),
            "focus-window 9".into(),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            let request = launch_worker::read_request(&mut stream);
            assert_eq!(request, format!("{command}\n"));
            stream.write_all(b"ok\n").unwrap();
            if command.starts_with("launch-app") {
                launch_worker::reply_active(&listener);
            }
        }
    });
    let mut worker = launch_worker::launch(&mut state);
    let mut ui = Ui::new(state, &build, (40, 48));
    let button = ui.button("Next app: Open Editor, pinned");
    ui.harness.focus_on(Some(button));
    for label in [
        "Open Editor, pinned",
        "Open Steam, pinned",
        "Switch to uninstalled",
    ] {
        assert_eq!(ui.button(&format!("Next app: {label}")), button);
        ui.harness
            .process_text_event(TextEvent::key_up(KeyboardKey::Named(NamedKey::Enter)));
        ui.dispatch(&build);
        worker.complete(&mut ui.state);
        ui.rebuild(&build);
        ui.dispatch(&build);
        assert_eq!(ui.harness.focused_widget_id(), Some(button));
    }
    ui.button("Next app: Open Editor, pinned");
    server.join().unwrap();
}

#[test]
fn a_large_catalog_cannot_evict_visible_pin_icons() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = fixture(dir.path());
    let applications = dir.path().join("applications");
    for index in 0..140 {
        std::fs::write(applications.join(format!("app-{index}.desktop")), format!(
            "[Desktop Entry]\nType=Application\nName=App {index}\nExec=unused\nIcon=aaa-{index}\n"
        )).unwrap();
    }
    state.catalog = AppCatalog::load_from_data_dirs(vec![dir.path().to_owned()]);
    state.favorites.store.add("steam_steam.desktop").unwrap();
    let requests = icons::requests(&state);
    assert_eq!(requests.len(), 128);
    let steam = state.catalog.app_by_id("steam_steam.desktop").unwrap();
    assert!(requests.contains(&icons::Request::app(steam)));
}
