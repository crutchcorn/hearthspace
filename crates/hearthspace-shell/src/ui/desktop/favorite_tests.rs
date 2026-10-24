//! The real desktop worker, persistent store, dock and launcher share this
//! boundary. Only compositor authorization is supplied by the socket fixture.
use super::*;
use crate::ui::{
    desktop::{context, context_actions},
    favorite_state::{Incoming, Mode},
    launcher, search_selection,
    test_support::Ui,
};
use hearthspace_desktop::favorites::{Point, Recognition};
use xilem::{WidgetView, view::sized_box};

const SHAPE: &[Point] = &[[30.0, 30.0], [30.0, 130.0], [130.0, 130.0]];

fn fixture(directory: &Path) -> ShellState {
    let applications = directory.join("applications");
    std::fs::create_dir(&applications).unwrap();
    for name in ["Alpha", "Editor", "Settings"] {
        std::fs::write(
            applications.join(format!("{}.desktop", name.to_lowercase())),
            format!("[Desktop Entry]\nType=Application\nName={name}\nExec=true\n"),
        )
        .unwrap();
    }
    let mut state = ShellState::new(
        directory.join("control.sock"),
        vec!["eDP-1".into(), "HDMI-A-1".into()],
    );
    state.catalog = AppCatalog::load_from_data_dirs(vec![directory.to_owned()]);
    state.favorites.store = FavoriteStore::load(directory.join("favorites.json")).unwrap();
    state
        .favorites
        .store
        .set_gesture("settings.desktop", SHAPE)
        .unwrap();
    state.outputs[0].show_search = true;
    state
}

fn change_favorite(state: &mut ShellState, generation: u64, add: bool) {
    let context = hearthspace_common::desktop_context::DesktopContext {
        generation,
        ids: vec![app().id],
        output: Some("eDP-1".into()),
        max_width: 340,
        max_height: 600,
    };
    context::apply(state, Some(context.clone()));
    state.desktop.menu.busy = true;
    let (sender, mut requests) = tokio::sync::mpsc::channel(4);
    state.desktop.requests = Some(sender);
    let listener = UnixListener::bind(&state.command_socket).unwrap();
    let server = std::thread::spawn(move || {
        reply(
            &listener,
            &format!("desktop-context-read {generation}"),
            &payload(&serde_json::json!({"context": context}).to_string()),
        );
    });
    let completed = perform(
        &state.command_socket,
        Request::Context(context_actions::Request {
            generation,
            action: context_actions::Action::Favorite(add),
        }),
        &[app()],
        state.favorites.store.clone(),
        false,
    )
    .unwrap();
    server.join().unwrap();
    std::fs::remove_file(&state.command_socket).unwrap();
    apply_completed(state, completed, true);
    assert!(state.desktop.menu.current.is_none());
    assert!(
        matches!(requests.try_recv().unwrap(), Request::Command(command)
        if command == format!("desktop-context-close {generation}"))
    );
}

fn view(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(launcher::launcher_view(
        state,
        &state.outputs[0],
        launcher::results(state, &state.outputs[0]),
        None,
    ))
}

fn app_order(state: &ShellState) -> Vec<String> {
    launcher::results(state, &state.outputs[0])
        .iter()
        .map(launcher::Entry::label)
        .collect()
}

#[test]
fn context_favorites_update_the_dock_without_reordering_the_launcher_or_losing_gestures() {
    let directory = tempfile::tempdir().unwrap();
    let mut ui = Ui::new(fixture(directory.path()), view, (650, 560));
    assert_eq!(app_order(&ui.state).len(), 3);
    ui.button("Open: Editor");
    assert_eq!(crate::ui::dock::desired_width(&ui.state), 44.0);
    ui.state.favorites.error = Some("Previous favorite write failed".into());
    change_favorite(&mut ui.state, 8, true);
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(
        app_order(&ui.state),
        ["Open: Alpha", "Open: Editor", "Open: Settings"]
    );
    assert!(ui.state.favorites.error.is_none());
    assert_eq!(crate::ui::dock::desired_width(&ui.state), 92.0);
    let favorite = ui.button("Open: Editor");
    let ordinary = ui.button("Open: Alpha");
    let favorite_bounds = ui
        .harness
        .access_node(favorite)
        .unwrap()
        .bounding_box()
        .unwrap();
    let ordinary_bounds = ui
        .harness
        .access_node(ordinary)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!(favorite_bounds.y0, ordinary_bounds.y0);
    assert!(ordinary_bounds.x1 <= favorite_bounds.x0);

    // Reopening the persistent store must retain the same order and old drawing.
    ui.state.favorites.store =
        FavoriteStore::load(directory.path().join("favorites.json")).unwrap();
    assert_eq!(
        app_order(&ui.state),
        ["Open: Alpha", "Open: Editor", "Open: Settings"]
    );
    assert_eq!(
        ui.state.favorites.store.recognize(SHAPE).unwrap(),
        Recognition::Match("settings.desktop".into())
    );
    ui.activate(ui.button("Show app list"));
    ui.dispatch(&view);
    let favorite = ui.button("Open: Editor");
    let ordinary = ui.button("Open: Alpha");
    let favorite_bounds = ui
        .harness
        .access_node(favorite)
        .unwrap()
        .bounding_box()
        .unwrap();
    let ordinary_bounds = ui
        .harness
        .access_node(ordinary)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert_eq!(favorite_bounds.x0, ordinary_bounds.x0);
    assert!(ordinary_bounds.y1 <= favorite_bounds.y0);
    search_selection::choose(
        &mut ui.state,
        "eDP-1",
        &search_selection::EntryId::App("editor.desktop".into()),
    );
    change_favorite(&mut ui.state, 9, false);
    assert_eq!(crate::ui::dock::desired_width(&ui.state), 44.0);
    search_selection::reconcile(&mut ui.state);
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(
        app_order(&ui.state),
        ["Open: Alpha", "Open: Editor", "Open: Settings"]
    );
    assert_eq!(
        ui.state.outputs[0].selected_result,
        Some(search_selection::EntryId::App("editor.desktop".into()))
    );
    ui.button("Open: Editor");
    ui.button("Open: Settings");
    let saved = FavoriteStore::load(directory.path().join("favorites.json")).unwrap();
    assert_eq!(saved.entries(), ui.state.favorites.store.entries());
    assert_eq!(
        saved.recognize(SHAPE).unwrap(),
        Recognition::Match("settings.desktop".into())
    );

    // Re-pinning preserves the retained list and grid controls.
    change_favorite(&mut ui.state, 10, true);
    ui.rebuild(&view);
    ui.dispatch(&view);
    ui.button("Open: Editor");
    ui.activate(ui.button("Show app grid"));
    ui.dispatch(&view);
    ui.button("Open: Editor");
}

#[test]
fn adding_a_favorite_preserves_another_outputs_in_flight_drawing() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = fixture(directory.path());
    state.outputs[1].show_search = true;
    let drawing = &mut state.outputs[1].drawing;
    drawing.start(Mode::Launch);
    drawing.preview = SHAPE[..2].to_vec();
    drawing.incoming = Some(Incoming {
        stroke: 7,
        next_sequence: 2,
        points: drawing.preview.clone(),
    });
    let generation = drawing.generation;

    change_favorite(&mut state, 8, true);
    crate::ui::favorite_events::reload(&mut state);

    let drawing = &state.outputs[1].drawing;
    assert_eq!(drawing.mode, Mode::Launch);
    assert_eq!(drawing.generation, generation);
    assert_eq!(drawing.preview, SHAPE[..2]);
    assert_eq!(drawing.incoming.as_ref().unwrap().next_sequence, 2);
    assert_eq!(
        state.favorites.store.recognize(SHAPE).unwrap(),
        Recognition::Match("settings.desktop".into())
    );
    assert!(launcher::results(&state, &state.outputs[1]).is_empty());
}

#[test]
fn missing_and_repeated_favorites_keep_all_installed_apps_once_and_search_stays_filtered() {
    let directory = tempfile::tempdir().unwrap();
    let mut state = fixture(directory.path());
    state.favorites.store.add("removed.desktop").unwrap();
    state.favorites.store.add("editor.desktop").unwrap();
    state.favorites.store.add("editor.desktop").unwrap();
    assert_eq!(
        app_order(&state),
        ["Open: Alpha", "Open: Editor", "Open: Settings"]
    );
    assert_eq!(
        launcher::results(&state, &state.outputs[1]).len(),
        state.catalog.apps().len()
    );
    state.outputs[0].query = "Alpha".into();
    assert_eq!(app_order(&state), ["Open: Alpha"]);
    state.outputs[0].query = "removed".into();
    assert!(app_order(&state).is_empty());
    state.outputs[0].query.clear();
    state.favorites.store.remove("editor.desktop").unwrap();
    state.favorites.store.remove("settings.desktop").unwrap();
    assert_eq!(
        app_order(&state),
        ["Open: Alpha", "Open: Editor", "Open: Settings"]
    );
}
