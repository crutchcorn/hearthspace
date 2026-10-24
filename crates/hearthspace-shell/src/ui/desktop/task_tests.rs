use super::worker::registration_commands;
use super::*;
use crate::desktop_items::DesktopItemKind;
use hearthspace_desktop::favorites::FavoriteStore;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
};

#[path = "favorite_tests.rs"]
mod favorites;

fn app() -> DesktopItem {
    DesktopItem {
        id: "app:editor.desktop".into(),
        title: "Editor".into(),
        kind: DesktopItemKind::App {
            app_id: "editor.desktop".into(),
            icon: None,
        },
    }
}

fn reply(listener: &UnixListener, expected: &str, response: &[u8]) {
    let (stream, _) = listener.accept().unwrap();
    let mut reader = BufReader::new(stream);
    let mut request = String::new();
    reader.read_line(&mut request).unwrap();
    assert_eq!(request.trim_end(), expected);
    reader.get_mut().write_all(response).unwrap();
}

fn payload(value: &str) -> Vec<u8> {
    format!("ok {}\n{value}", value.len()).into_bytes()
}

#[test]
fn accepted_desktop_activation_uses_the_existing_app_launch_command() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        reply(
            &listener,
            "desktop-event-accept 7",
            &payload(r#"{"action":"activate","id":"app:editor.desktop"}"#),
        );
        reply(&listener, "launch-app editor.desktop", b"ok\n");
        reply(&listener, "desktop-clear", b"ok\n");
    });
    assert!(matches!(
        perform(
            &socket,
            Request::Accept(7),
            &[app()],
            FavoriteStore::in_memory(),
            false
        )
        .unwrap(),
        Completed::Activated
    ));
    server.join().unwrap();
}

#[test]
fn favorite_drop_persists_without_losing_existing_shortcuts() {
    let directory = tempfile::tempdir().unwrap();
    let favorites_path = directory.path().join("favorites.json");
    let mut favorites = FavoriteStore::load(&favorites_path).unwrap();
    favorites.add("settings.desktop").unwrap();
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        reply(
            &listener,
            "desktop-event-accept 8",
            &payload(r#"{"action":"favorite","id":"app:editor.desktop"}"#),
        );
    });
    let Completed::Favorite(updated) =
        perform(&socket, Request::Accept(8), &[app()], favorites, false).unwrap()
    else {
        panic!("favorite was not saved");
    };
    server.join().unwrap();
    assert_eq!(updated.entries().len(), 2);
    assert_eq!(
        FavoriteStore::load(&favorites_path).unwrap().entries(),
        updated.entries()
    );
}

#[test]
fn rejected_authorization_does_not_write_favorites() {
    let directory = tempfile::tempdir().unwrap();
    let favorites_path = directory.path().join("favorites.json");
    let mut favorites = FavoriteStore::load(&favorites_path).unwrap();
    favorites.add("settings.desktop").unwrap();
    let before = std::fs::read(&favorites_path).unwrap();
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        reply(&listener, "desktop-event-accept 9", b"err expired token\n")
    });
    let failure = perform(&socket, Request::Accept(9), &[app()], favorites, false).unwrap_err();
    assert!(failure.message.contains("expired"));
    assert!(!failure.report);
    server.join().unwrap();
    assert_eq!(std::fs::read(&favorites_path).unwrap(), before);
}

#[test]
fn unavailable_desktop_directory_preserves_files_and_initial_application_shortcuts() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("share");
    std::fs::create_dir_all(data.join("applications")).unwrap();
    std::fs::write(
        data.join("applications/editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=true\n",
    )
    .unwrap();
    let catalog = AppCatalog::load_from_data_dirs(vec![data]);
    let previous_file = DesktopItem {
        id: "file:61".into(),
        title: "a".into(),
        kind: DesktopItemKind::File {
            path: directory.path().join("a"),
        },
    };
    let unavailable = directory.path().join("not-directory");
    std::fs::write(&unavailable, "content").unwrap();
    let (_, items, error) = discover(
        catalog,
        Some(&unavailable),
        std::slice::from_ref(&previous_file),
    );
    assert!(error.is_some());
    assert_eq!(items[0], previous_file);
    assert_eq!(items[1].id, "app:editor.desktop");
}

#[test]
fn desktop_overflow_bounds_surfaces_and_keeps_the_complete_launcher_catalog() {
    use hearthspace_common::MAX_DESKTOP_ITEMS;

    let directory = tempfile::tempdir().unwrap();
    let desktop = directory.path().join("Desktop");
    std::fs::create_dir(&desktop).unwrap();
    for index in 0..=MAX_DESKTOP_ITEMS {
        std::fs::write(desktop.join(format!("Item-{index:04}.txt")), "").unwrap();
    }
    let data = directory.path().join("share");
    std::fs::create_dir_all(data.join("applications")).unwrap();
    std::fs::write(
        data.join("applications/editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=true\n",
    )
    .unwrap();
    let catalog = AppCatalog::load_from_data_dirs(vec![data]);

    let (catalog, items, error) = discover(catalog, Some(&desktop), &[]);

    assert_eq!(items.len(), MAX_DESKTOP_ITEMS);
    assert_eq!(items[0].title, "Item-0000.txt");
    assert_eq!(
        items.last().unwrap().title,
        format!("Item-{:04}.txt", MAX_DESKTOP_ITEMS - 1)
    );
    assert!(catalog.app_by_id("editor.desktop").is_some());
    let error = error.unwrap();
    assert!(error.contains(&format!(
        "Showing {MAX_DESKTOP_ITEMS} of {}",
        MAX_DESKTOP_ITEMS + 2
    )));
    assert!(error.contains("Apps List"));
}

#[test]
fn large_catalog_registration_is_batched_without_reordering_or_losing_ids() {
    let ids: Vec<_> = (0..400)
        .map(|index| format!("app:{index:04}{}.desktop", "x".repeat(110)))
        .collect();
    let commands = registration_commands(&ids);
    assert!(commands.len() > 1);
    let mut received = Vec::new();
    for command in commands {
        assert!(command.len() < hearthspace_ipc::transport::MAX_REQUEST_BYTES);
        let value: serde_json::Value =
            serde_json::from_str(command.strip_prefix("desktop-register ").unwrap()).unwrap();
        assert_eq!(value["offset"].as_u64().unwrap() as usize, received.len());
        received.extend(
            value["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_owned()),
        );
    }
    assert_eq!(received, ids);
}

#[test]
fn favorite_completion_updates_shared_state_without_changing_a_replacement_menu() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    super::super::context::apply(
        &mut state,
        Some(hearthspace_common::desktop_context::DesktopContext {
            generation: 9,
            ids: vec![],
            output: Some("eDP-1".into()),
            max_width: 340,
            max_height: 600,
        }),
    );
    state.desktop.menu.busy = true;
    state.favorites.error = Some("old write failure".into());
    let (sender, mut requests) = tokio::sync::mpsc::channel(4);
    state.desktop.requests = Some(sender);
    let mut saved = FavoriteStore::in_memory();
    saved.add("editor.desktop").unwrap();

    apply_completed(
        &mut state,
        Completed::Context(
            8,
            Ok(super::super::context_actions::Completed::Closed(Some(
                saved,
            ))),
        ),
        true,
    );

    assert_eq!(state.favorites.store.entries()[0].app_id, "editor.desktop");
    assert!(state.favorites.error.is_none());
    assert_eq!(state.desktop.menu.current.as_ref().unwrap().generation, 9);
    assert!(state.desktop.menu.busy);
    assert!(requests.try_recv().is_err());
}

#[test]
fn favorite_completion_updates_shared_state_without_reopening_an_inactive_menu() {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    let mut saved = FavoriteStore::in_memory();
    saved.add("editor.desktop").unwrap();

    apply_completed(
        &mut state,
        Completed::Context(
            8,
            Ok(super::super::context_actions::Completed::Closed(Some(
                saved,
            ))),
        ),
        false,
    );

    assert_eq!(state.favorites.store.entries()[0].app_id, "editor.desktop");
    assert!(state.desktop.menu.current.is_none());
}

#[path = "launch_tests.rs"]
mod launches;
