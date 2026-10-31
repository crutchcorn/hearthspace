use super::*;
use hearthspace_desktop::app_catalog::AppCatalog;
use masonry::accesskit::Toggled;

const LABEL: &str = "Run using dedicated graphics card";

#[path = "gpu_keyboard.rs"]
mod keyboard;

#[path = "gpu_layout.rs"]
mod layout;

fn catalog(directory: &std::path::Path, enabled: bool) -> AppCatalog {
    std::fs::create_dir_all(directory.join("applications")).unwrap();
    std::fs::write(
        directory.join("applications/editor.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=true\nPrefersNonDefaultGPU={enabled}\n"
        ),
    )
    .unwrap();
    AppCatalog::load_from_data_dirs(vec![directory.to_owned()])
}

fn app_state(
    directory: &std::path::Path,
    enabled: bool,
) -> (ShellState, tokio::sync::mpsc::Receiver<task::Request>) {
    let (mut state, requests) = state(true, 600);
    state.catalog = catalog(directory, enabled);
    state.desktop.replace_items(vec![DesktopItem {
        id: "app:editor.desktop".into(),
        title: "Editor".into(),
        kind: DesktopItemKind::App {
            app_id: "editor.desktop".into(),
            icon: None,
        },
    }]);
    state.desktop.menu.current.as_mut().unwrap().ids = vec!["app:editor.desktop".into()];
    (state, requests)
}

#[test]
fn saved_gpu_preference_stays_checked_while_saving_and_failed_saves_can_retry() {
    let directory = tempfile::tempdir().unwrap();
    let (state, mut requests) = app_state(directory.path(), true);
    let mut ui = Ui::new(state, panel, (340, 360));
    let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
    let node = ui.harness.access_node(checkbox).unwrap();
    assert_eq!(node.data().toggled(), Some(Toggled::True));
    assert_eq!(node.data().description(), Some(rows::GPU_HELP));
    ui.activate(checkbox);
    ui.dispatch(&panel);
    let task::Request::Context(request) = requests.try_recv().unwrap() else {
        panic!("missing preference request");
    };
    assert!(matches!(request.action, Action::GpuPreference(false)));
    assert!(ui.state.desktop.menu.busy);
    assert!(ui.state.desktop.menu.saving_gpu);
    assert_eq!(
        ui.harness.access_node(checkbox).unwrap().data().toggled(),
        Some(Toggled::True)
    );
    assert!(
        ui.harness
            .access_node(checkbox)
            .unwrap()
            .data()
            .is_disabled()
    );
    named(&ui, "Graphics preference", Role::Status);
    finished(
        &mut ui.state,
        8,
        Err("Could not save graphics preference. Try again.".into()),
    );
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert_eq!(ui.harness.focused_widget_id(), Some(checkbox));
    named(&ui, "Desktop action", Role::Status);
    // Retry via a full keyboard sequence, keeping the saved value until completion.
    press(&mut ui, NamedKey::Enter);
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&panel);
    assert!(matches!(
        requests.try_recv().unwrap(),
        task::Request::Context(context_actions::Request {
            action: Action::GpuPreference(false),
            ..
        })
    ));
    let saved = catalog(directory.path(), false);
    ui.state.catalog = saved.clone();
    finished(&mut ui.state, 8, Ok(Completed::GpuPreference(saved)));
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert!(!ui.state.desktop.menu.busy);
    assert!(ui.state.desktop.menu.saved_gpu);
    assert!(ui.state.desktop.menu.error.is_none());
    assert_eq!(
        ui.harness.access_node(checkbox).unwrap().data().toggled(),
        Some(Toggled::False)
    );
    assert!(ui.state.desktop.menu.current.is_some());
}

#[test]
fn gpu_preference_exists_without_capability_gating_and_busy_escape_does_not_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let (state, mut requests) = app_state(directory.path(), false);
    let mut ui = Ui::new(state, panel, (340, 360));
    let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
    ui.activate(checkbox);
    ui.dispatch(&panel);
    requests.try_recv().unwrap();
    let snapshot = ui.state.desktop.menu.current.clone();
    press(&mut ui, NamedKey::Escape);
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Escape)));
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.current.is_none());
    apply(&mut ui.state, snapshot);
    finished(
        &mut ui.state,
        8,
        Ok(Completed::GpuPreference(catalog(directory.path(), true))),
    );
    assert!(ui.state.desktop.menu.current.is_none());
    assert!(
        matches!(requests.try_recv().unwrap(), task::Request::Command(command)
        if command == "desktop-context-close 8")
    );
    assert!(requests.try_recv().is_err());
}

#[test]
fn unavailable_worker_keeps_saved_gpu_value_and_a_retryable_menu() {
    let directory = tempfile::tempdir().unwrap();
    let (state, requests) = app_state(directory.path(), true);
    drop(requests);
    let mut ui = Ui::new(state, panel, (340, 360));
    let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
    ui.activate(checkbox);
    ui.dispatch(&panel);
    assert!(!ui.state.desktop.menu.busy);
    assert!(ui.state.desktop.menu.current.is_some());
    assert_eq!(
        ui.harness.access_node(checkbox).unwrap().data().toggled(),
        Some(Toggled::True)
    );
    named(&ui, "Desktop action", Role::Status);
}

#[test]
fn catalog_app_menu_keeps_all_app_actions_when_file_tiles_fill_the_desktop() {
    let directory = tempfile::tempdir().unwrap();
    let (mut state, mut requests) = app_state(directory.path(), true);
    state.favorites.store = hearthspace_desktop::favorites::FavoriteStore::in_memory();
    state.desktop.replace_items(
        (0..hearthspace_common::MAX_DESKTOP_ITEMS)
            .map(|index| DesktopItem {
                id: format!("file:{index:08x}"),
                title: format!("File {index}"),
                kind: DesktopItemKind::File {
                    path: format!("/fixture/Desktop/{index}").into(),
                },
            })
            .collect(),
    );
    assert!(
        state
            .desktop
            .items
            .iter()
            .all(|item| !matches!(item.content.kind, DesktopItemKind::App { .. }))
    );
    let mut ui = Ui::new(state, panel, (340, 360));
    for label in ["Open", "Add to Favorites", "Properties"] {
        named(&ui, label, Role::MenuItem);
    }
    let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
    assert_eq!(
        ui.harness.access_node(checkbox).unwrap().toggled(),
        Some(Toggled::True)
    );
    assert!(!ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            matches!(
                node.label().as_deref(),
                Some("Rename…" | "Move to Trash" | "New Folder…" | "Paste")
            )
        })
    }));
    ui.activate(checkbox);
    ui.dispatch(&panel);
    assert!(matches!(
        requests.try_recv().unwrap(),
        task::Request::Context(context_actions::Request {
            generation: 8,
            action: Action::GpuPreference(false),
        })
    ));
    assert!(requests.try_recv().is_err());
    finished(&mut ui.state, 8, Err("Try again.".into()));
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert_eq!(ui.harness.focused_widget_id(), Some(checkbox));
    // Uninstalling the app cannot turn this app menu into canvas/file actions.
    ui.state.catalog = AppCatalog::default();
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    named(&ui, "Close", Role::MenuItem);
    assert!(rows::gpu_preference(&ui.state).is_none());
    assert_eq!(rows::rows(&ui.state, false).len(), 1);
}
