use super::*;
use crate::ui::desktop::{context, context_actions};

#[test]
fn saved_gpu_completion_updates_catalog_without_changing_a_replacement_menu() {
    let directory = tempfile::tempdir().unwrap();
    let apps = directory.path().join("applications");
    std::fs::create_dir(&apps).unwrap();
    std::fs::write(
        apps.join("editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=true\nPrefersNonDefaultGPU=true\n",
    )
    .unwrap();
    let catalog = AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()]);
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    context::apply(
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
    apply_completed(
        &mut state,
        Completed::Context(
            8,
            Ok(context_actions::Completed::GpuPreference(catalog.clone())),
        ),
        true,
    );
    assert!(
        state
            .catalog
            .app_by_id("editor.desktop")
            .unwrap()
            .prefers_non_default_gpu
    );
    assert_eq!(state.desktop.menu.current.as_ref().unwrap().generation, 9);
    assert!(state.desktop.menu.busy);
    context::apply(&mut state, None);
    apply_completed(
        &mut state,
        Completed::Context(8, Ok(context_actions::Completed::GpuPreference(catalog))),
        false,
    );
    assert!(
        state
            .catalog
            .app_by_id("editor.desktop")
            .unwrap()
            .prefers_non_default_gpu
    );
    assert!(state.desktop.menu.current.is_none());
}

#[test]
fn rejected_gpu_action_authorization_returns_to_menu_without_persisting() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        reply(
            &listener,
            "desktop-context-read 8",
            b"err expired context\n",
        );
    });
    let result = perform(
        &socket,
        Request::Context(context_actions::Request {
            generation: 8,
            action: context_actions::Action::GpuPreference(true),
        }),
        &[app()],
        &AppCatalog::default(),
        FavoriteStore::in_memory(),
        false,
        &mut AppRegistration::default(),
    )
    .unwrap();
    server.join().unwrap();
    assert!(
        matches!(result, Completed::Context(8, Err(message)) if message.contains("expired context"))
    );
}
