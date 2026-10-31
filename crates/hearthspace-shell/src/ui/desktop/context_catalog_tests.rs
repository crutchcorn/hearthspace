//! Real context actions resolve installed apps even when every tile is a file.
use super::*;
use crate::ui::desktop::{context_actions, context_items};
use context_actions::Action;

fn catalog(directory: &Path) -> AppCatalog {
    std::fs::create_dir_all(directory.join("applications")).unwrap();
    std::fs::write(
        directory.join("applications/editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=true\nPrefersNonDefaultGPU=false\n",
    )
    .unwrap();
    AppCatalog::load_from_data_dirs(vec![directory.to_owned()])
}

fn files() -> Vec<DesktopItem> {
    (0..hearthspace_common::MAX_DESKTOP_ITEMS)
        .map(|index| DesktopItem {
            id: format!("file:{index:08x}"),
            title: format!("File {index}"),
            kind: DesktopItemKind::File {
                path: format!("/fixture/Desktop/{index}").into(),
            },
        })
        .collect()
}

fn run(
    socket: &Path,
    id: &str,
    action: Action,
    catalog: &AppCatalog,
    known: &[DesktopItem],
) -> Result<context_actions::Completed, String> {
    let context = hearthspace_common::desktop_context::DesktopContext {
        generation: 42,
        ids: vec![id.into()],
        output: None,
        max_width: 340,
        max_height: 600,
    };
    let open = matches!(action, Action::Open) && catalog.app_by_id("editor.desktop").is_some();
    let listener = UnixListener::bind(socket).unwrap();
    let server = std::thread::spawn(move || {
        reply(
            &listener,
            "desktop-context-read 42",
            &payload(&serde_json::json!({"context": context}).to_string()),
        );
        if open {
            reply(&listener, "launch-app editor.desktop", b"ok\n");
        }
    });
    let completed = perform(
        socket,
        Request::Context(context_actions::Request {
            generation: 42,
            action,
        }),
        known,
        catalog,
        FavoriteStore::in_memory(),
        false,
        &mut AppRegistration::default(),
    )
    .unwrap();
    server.join().unwrap();
    std::fs::remove_file(socket).unwrap();
    let Completed::Context(42, result) = completed else {
        panic!("expected context completion");
    };
    result
}

#[test]
fn catalog_only_app_can_open_and_change_favorites_with_a_full_file_tile_list() {
    let directory = tempfile::tempdir().unwrap();
    let catalog = catalog(directory.path());
    let socket = directory.path().join("control.sock");
    let files = files();
    assert!(matches!(
        run(
            &socket,
            "app:editor.desktop",
            Action::Open,
            &catalog,
            &files
        )
        .unwrap(),
        context_actions::Completed::Closed(None)
    ));
    let context_actions::Completed::Closed(Some(favorites)) = run(
        &socket,
        "app:editor.desktop",
        Action::Favorite(true),
        &catalog,
        &files,
    )
    .unwrap() else {
        panic!("favorite did not persist");
    };
    assert_eq!(favorites.entries()[0].app_id, "editor.desktop");
    // App catalog membership never allows file-only operations on its entry.
    assert!(
        run(
            &socket,
            "app:editor.desktop",
            Action::Copy,
            &catalog,
            &files
        )
        .unwrap_err()
        .contains("files and folders")
    );
}

#[test]
fn removed_app_cannot_fall_back_to_its_old_tile_or_gain_file_authority() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("control.sock");
    for action in [
        Action::Open,
        Action::Favorite(true),
        Action::GpuPreference(true),
        Action::Properties,
    ] {
        let error = run(
            &socket,
            "app:editor.desktop",
            action,
            &AppCatalog::default(),
            &[app()],
        )
        .unwrap_err();
        assert!(error.contains("no longer available"), "{error}");
    }
    let catalog = catalog(directory.path());
    assert!(context_items::resolve("file:missing", &catalog, &[]).is_none());
    let known = files();
    assert_eq!(
        context_items::resolve(&known[0].id, &catalog, &known),
        Some(known[0].clone())
    );
}

#[test]
fn catalog_only_gpu_action_saves_in_an_isolated_environment() {
    use crate::desktop_items::test_support::{Fixture, assert_success, child_root};
    const TEST: &str = "ui::desktop::task::tests::context_catalog::catalog_only_gpu_action_saves_in_an_isolated_environment";
    if let Some(root) = child_root() {
        let catalog = AppCatalog::load();
        let source = catalog.app_by_id("editor.desktop").unwrap().path.clone();
        let original = std::fs::read(&source).unwrap();
        let saved = run(
            &root.join("control.sock"),
            "app:editor.desktop",
            Action::GpuPreference(true),
            &catalog,
            &files(),
        )
        .unwrap();
        let context_actions::Completed::GpuPreference(catalog) = saved else {
            panic!("GPU preference did not save");
        };
        let app = catalog.app_by_id("editor.desktop").unwrap();
        assert!(app.prefers_non_default_gpu);
        assert_eq!(app.path, root.join("share/applications/editor.desktop"));
        assert_eq!(std::fs::read(source).unwrap(), original);
        return;
    }
    let fixture = Fixture::new();
    catalog(&fixture.path().join("system"));
    let mut child = fixture.child(TEST);
    child.env("XDG_DATA_DIRS", fixture.path().join("system"));
    assert_success(&mut child);
}
