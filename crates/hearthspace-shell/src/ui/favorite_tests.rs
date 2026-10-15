use super::{
    favorite_actions as actions, favorite_state::Mode, launcher, state::ShellState,
    test_support::Ui,
};
use crate::{app_catalog::AppCatalog, favorites::FavoriteStore};
use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    thread,
};
use xilem::{WidgetView, view::sized_box};

const SHAPE: [[f64; 2]; 3] = [[30.0, 30.0], [30.0, 130.0], [130.0, 130.0]];

fn state(dir: &std::path::Path) -> ShellState {
    let apps = dir.join("applications");
    std::fs::create_dir_all(&apps).unwrap();
    std::fs::write(
        apps.join("fixture.desktop"),
        "[Desktop Entry]\nType=Application\nName=Fixture App\nExec=true\n",
    )
    .unwrap();
    let mut state = ShellState::new(dir.join("control.sock"), vec!["eDP-1".into()]);
    state.catalog = AppCatalog::load_from_data_dirs(vec![dir.to_owned()]);
    state.favorites.store = FavoriteStore::load(dir.join("favorites.json")).unwrap();
    state.toggle_search("eDP-1");
    state
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

#[test]
fn search_exposes_favorites_and_drawing_but_configuration_lives_in_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.favorites.store.add("fixture.desktop").unwrap();
    state
        .favorites
        .store
        .assign("fixture.desktop", &SHAPE)
        .unwrap();
    let mut ui = Ui::new(state, view, (768, 560));
    ui.button("Open: Fixture App");
    ui.button("Manage favorites");
    ui.activate(ui.button("Draw to launch"));
    ui.dispatch(&view);
    assert_eq!(ui.state.outputs[0].drawing.mode, Mode::Launch);
    assert!(launcher::results(&ui.state, &ui.state.outputs[0]).is_empty());
    ui.activate(ui.button("Back to search"));
    ui.dispatch(&view);
    assert_eq!(ui.state.outputs[0].drawing.mode, Mode::Browse);
}

#[test]
fn clear_match_launches_immediately_by_id_and_launch_failure_keeps_feedback() {
    for accepted in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let mut state = state(dir.path());
        state.favorites.store.add("fixture.desktop").unwrap();
        state
            .favorites
            .store
            .assign("fixture.desktop", &SHAPE)
            .unwrap();
        let listener = UnixListener::bind(&state.command_socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).unwrap();
            assert!(request.contains("fixture.desktop"));
            assert!(
                !request.contains("true"),
                "launch must use catalog ID, never a stored command"
            );
            stream
                .write_all(if accepted {
                    b"ok\n"
                } else {
                    b"err fixture launch failed\n"
                })
                .unwrap();
        });
        actions::start(&mut state, "eDP-1", Mode::Launch);
        actions::complete(
            &mut state,
            "eDP-1",
            vec![[80.0, 20.0], [80.0, 100.0], [160.0, 100.0]],
            1,
            1,
        );
        assert_eq!(state.outputs[0].show_search, !accepted);
        assert_eq!(state.outputs[0].launch_error.is_some(), !accepted);
        server.join().unwrap();
    }
}

#[test]
fn missing_apps_and_unclear_strokes_do_not_launch_or_destroy_favorites() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.favorites.store.add("removed.desktop").unwrap();
    state
        .favorites
        .store
        .assign("removed.desktop", &SHAPE)
        .unwrap();
    actions::start(&mut state, "eDP-1", Mode::Launch);
    actions::complete(&mut state, "eDP-1", SHAPE.to_vec(), 1, 1);
    assert!(
        state.outputs[0]
            .drawing
            .message
            .as_ref()
            .unwrap()
            .contains("no longer installed")
    );
    actions::complete(&mut state, "eDP-1", vec![[1.0, 1.0], [2.0, 2.0]], 1, 2);
    assert!(state.outputs[0].show_search);
    assert_eq!(state.favorites.store.entries().len(), 1);
    actions::cancel(&mut state, "eDP-1");
    let ui = Ui::new(state, view, (768, 560));
    ui.button("Manage favorites");
}

#[test]
fn closing_changing_query_and_switching_panels_cancel_drawing() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    for close in [
        ShellState::dismiss_search,
        ShellState::toggle_map,
        ShellState::dismiss_launcher,
    ] {
        state.change_query("eDP-1", "".into());
        actions::start(&mut state, "eDP-1", Mode::Launch);
        close(&mut state, "eDP-1");
        assert!(!state.outputs[0].drawing.active());
    }
    state.change_query("eDP-1", "".into());
    actions::start(&mut state, "eDP-1", Mode::Launch);
    assert!(launcher::results(&state, &state.outputs[0]).is_empty());
    state.change_query("eDP-1", "Fixture".into());
    assert!(!state.outputs[0].drawing.active());
}

#[test]
fn empty_favorites_offer_settings_and_disable_drawing() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let ui = Ui::new(state, view, (768, 560));
    assert!(
        ui.harness
            .access_node(ui.button("Draw to launch"))
            .unwrap()
            .is_disabled()
    );
    ui.button("Manage favorites");
}
