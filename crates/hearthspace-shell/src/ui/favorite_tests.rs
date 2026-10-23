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
fn apps_and_draw_tabs_preserve_browsing_and_expose_gesture_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.favorites.store.add("fixture.desktop").unwrap();
    state
        .favorites
        .store
        .assign("fixture.desktop", &SHAPE)
        .unwrap();
    let mut ui = Ui::new(state, view, (768, 560));
    assert_eq!(launcher::results(&ui.state, &ui.state.outputs[0]).len(), 1);
    ui.button("Open: Fixture App, favorite");
    let editor = ui.harness.focused_widget_id().unwrap();
    let bounds = |id| ui.harness.access_node(id).unwrap().bounding_box().unwrap();
    let input = bounds(editor);
    let draw = bounds(ui.button("Draw to launch"));
    assert!(draw.x0 >= input.x1);
    assert!(draw.y0 <= input.y1 && draw.y1 >= input.y0);
    ui.activate(ui.button("Draw to launch"));
    ui.dispatch(&view);
    assert_eq!(ui.state.outputs[0].drawing.mode, Mode::Launch);
    ui.button("Settings");
    assert!(launcher::results(&ui.state, &ui.state.outputs[0]).is_empty());
    ui.activate(ui.button("Back to apps list"));
    ui.dispatch(&view);
    assert_eq!(ui.state.outputs[0].drawing.mode, Mode::Browse);
    ui.button("Open: Fixture App, favorite");
}

#[test]
fn live_drawing_replaces_prompt_and_keeps_feedback_controls_usable_after_resize() {
    use super::favorite_events::{self, GestureEvent, Phase};
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.favorites.store.add("fixture.desktop").unwrap();
    state
        .favorites
        .store
        .assign("fixture.desktop", &SHAPE)
        .unwrap();
    let mut ui = Ui::new(state, view, (650, 560));
    ui.activate(ui.button("Draw to launch"));
    ui.dispatch(&view);
    let generation = ui.state.outputs[0].drawing.generation;
    // The test boundary supplies compositor authorization after the real UI
    // has measured its pad. Subsequent events follow the normal gesture path.
    ui.state.outputs[0].drawing.armed = true;
    ui.state.outputs[0].drawing.message = None;
    ui.rebuild(&view);
    ui.dispatch(&view);
    let initial_height = super::popup::size(
        &ui.state.outputs[0],
        super::popup::PopupKind::Search,
        650.0,
        560.0,
    )
    .height;
    let listener = UnixListener::bind(&ui.state.command_socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        stream.read_to_string(&mut request).unwrap();
        assert_eq!(request, format!("favorite-gesture-accept {generation} 1\n"));
        stream.write_all(b"ok\n").unwrap();
    });
    for (sequence, phase, point) in [
        (0, Phase::Begin, [30.0, 30.0]),
        (1, Phase::Update, [100.0, 30.0]),
        (2, Phase::Update, [30.0, 30.0]),
        (3, Phase::End, [30.0, 30.0]),
    ] {
        favorite_events::apply(
            &mut ui.state,
            GestureEvent {
                generation,
                stroke: 1,
                sequence,
                phase,
                point,
                output: Some("eDP-1".into()),
            },
        );
        ui.rebuild(&view);
        ui.dispatch(&view);
        let height = super::popup::size(
            &ui.state.outputs[0],
            super::popup::PopupKind::Search,
            650.0,
            560.0,
        )
        .height as u32;
        ui.harness
            .process_window_event(masonry::core::WindowEvent::Resize((650, height).into()));
        ui.dispatch(&view);
        ui.harness.render();
    }
    server.join().unwrap();
    assert_eq!(
        super::popup::size(
            &ui.state.outputs[0],
            super::popup::PopupKind::Search,
            650.0,
            560.0,
        )
        .height,
        initial_height,
        "drawing feedback must not reconfigure and cancel its own popup"
    );
    assert!(
        ui.state.outputs[0]
            .drawing
            .message
            .as_ref()
            .unwrap()
            .contains("No matching")
    );
    let retry = ui.button("Clear drawing and retry");
    ui.harness.focus_on(Some(retry));
    ui.dispatch(&view);
    assert_eq!(ui.harness.focused_widget_id(), Some(retry));
    ui.activate(retry);
    ui.dispatch(&view);
    assert!(ui.state.outputs[0].drawing.preview.is_empty());
    assert!(ui.state.outputs[0].drawing.generation > generation);
    assert!(super::test_support::ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.value().as_deref() == Some("Draw a gesture")
                || node.label().as_deref() == Some("Draw a gesture")
        })
    }));
    ui.button("Settings");
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
    let mut ui = Ui::new(state, view, (768, 560));
    ui.activate(ui.button("Draw to launch"));
    ui.dispatch(&view);
    ui.button("Settings");
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
fn empty_gestures_keep_drawing_available_and_explain_where_to_assign_one() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let mut ui = Ui::new(state, view, (768, 560));
    assert!(
        !ui.harness
            .access_node(ui.button("Draw to launch"))
            .unwrap()
            .is_disabled()
    );
    ui.activate(ui.button("Draw to launch"));
    ui.dispatch(&view);
    assert!(super::test_support::ids(&ui.harness).into_iter().any(|id| {
        ui.harness.access_node(id).is_some_and(|node| {
            node.value().as_deref() == Some("Add an app gesture in Settings → Gestures.")
                || node.label().as_deref() == Some("Add an app gesture in Settings → Gestures.")
        })
    }));
    ui.activate(ui.button("Back to apps list"));
    ui.dispatch(&view);
    assert!(!ui.state.outputs[0].drawing.active());
}

#[test]
fn one_finger_scrolls_real_search_rows_without_launching_or_changing_query() {
    use super::window_catalog::RunningWindow;
    use masonry::core::{
        PointerButtonEvent, PointerEvent, PointerInfo, PointerState, PointerType, PointerUpdate,
        pointer::PointerId,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.windows = (1..=4)
        .map(|id| RunningWindow {
            id,
            title: format!("Notes {id}"),
            app_id: "notes".into(),
            focused: false,
        })
        .collect();
    state.change_query("eDP-1", "Notes".into());
    let mut ui = Ui::new(state, view, (768, 260));
    let first = ui.button("Window: Notes 1");
    let before = ui
        .harness
        .access_node(first)
        .unwrap()
        .bounding_box()
        .unwrap();
    let start = ui
        .harness
        .access_node(first)
        .unwrap()
        .bounding_box()
        .unwrap();
    let pointer = PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Touch,
    };
    let origin = PointerState {
        position: ((start.x0 + start.x1) / 2.0, (start.y0 + start.y1) / 2.0).into(),
        pressure: 0.5,
        ..Default::default()
    };
    let end = PointerState {
        position: (origin.position.x, origin.position.y - 44.0).into(),
        ..origin.clone()
    };
    for event in [
        PointerEvent::Down(PointerButtonEvent {
            pointer,
            state: origin,
            button: None,
        }),
        PointerEvent::Move(PointerUpdate {
            pointer,
            current: end.clone(),
            coalesced: vec![],
            predicted: vec![],
        }),
        PointerEvent::Up(PointerButtonEvent {
            pointer,
            state: end,
            button: None,
        }),
    ] {
        ui.harness.process_pointer_event(event);
        ui.dispatch(&view);
    }
    let after = ui
        .harness
        .access_node(first)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(
        after.y0 < before.y0 - 20.0,
        "result did not scroll: {before:?} -> {after:?}"
    );
    assert!(ui.state.outputs[0].show_search);
    assert!(ui.state.outputs[0].launch_error.is_none());
    assert_eq!(ui.state.outputs[0].query, "Notes");
}
