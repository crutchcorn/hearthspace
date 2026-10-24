//! Render while real launch transport waits, so feedback cannot move the pad.
use super::*;
use crate::ui::{
    favorite_events::{self, GestureEvent, Phase},
    launch::tests::worker,
    popup::{self, PopupKind},
};
use std::{sync::mpsc, time::Duration};

#[test]
fn pending_drawing_launch_keeps_bounds_and_generation_until_acceptance() {
    drawing_launch(650, true);
}

#[test]
fn rejected_drawing_launch_keeps_visible_failure_on_a_narrow_output() {
    drawing_launch(272, false);
}

fn drawing_launch(width: u32, accepted: bool) {
    let dir = tempfile::tempdir().unwrap();
    let mut state = state(dir.path());
    state.favorites.store.add("fixture.desktop").unwrap();
    state
        .favorites
        .store
        .assign("fixture.desktop", &SHAPE)
        .unwrap();
    let mut ui = Ui::new(state, view, (width, 560));
    ui.activate(ui.button("Draw to launch"));
    ui.dispatch(&view);
    for _ in 0..2 {
        let height = popup::size(
            &ui.state.outputs[0],
            PopupKind::Search,
            f64::from(width),
            560.0,
        )
        .height;
        ui.harness
            .process_window_event(masonry::core::WindowEvent::Resize(
                (width, height as u32).into(),
            ));
        ui.dispatch(&view);
    }
    // Only compositor authorization is supplied by this fixture. Layout,
    // ordered stroke handling, recognition, task, transport and reply are real.
    ui.state.outputs[0].drawing.armed = true;
    ui.state.outputs[0].drawing.message = None;
    ui.rebuild(&view);
    ui.dispatch(&view);
    let generation = ui.state.outputs[0].drawing.generation;
    let bounds = ui.state.outputs[0].drawing.bounds.unwrap();
    let height = popup::size(
        &ui.state.outputs[0],
        PopupKind::Search,
        f64::from(width),
        560.0,
    )
    .height;
    let mut launch = worker::launch(&mut ui.state);
    let listener = UnixListener::bind(&ui.state.command_socket).unwrap();
    let (entered, pending) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let endpoint = thread::spawn(move || {
        let (mut stream, line) = worker::accept(&listener);
        let envelope = hearthspace_ipc::transport::launch::Envelope::decode(line.as_bytes())
            .unwrap()
            .expect("drawing launch must retain caller lifetime");
        assert_eq!(
            envelope.command,
            format!("favorite-gesture-launch {generation} 7 \"fixture.desktop\"")
        );
        entered.send(()).unwrap();
        held.recv_timeout(Duration::from_secs(3)).unwrap();
        stream
            .write_all(if accepted {
                b"ok\n"
            } else {
                b"err fixture launch failed\n"
            })
            .unwrap();
        worker::reply_active(&listener);
        if accepted {
            let (mut stream, request) = worker::accept(&listener);
            assert_eq!(request, format!("favorite-gesture-disarm {generation}\n"));
            stream.write_all(b"ok\n").unwrap();
        }
    });
    for (sequence, phase, point) in [
        (0, Phase::Begin, [80.0, 20.0]),
        (1, Phase::Update, [80.0, 100.0]),
        (2, Phase::Update, [160.0, 100.0]),
        (3, Phase::End, [160.0, 100.0]),
    ] {
        favorite_events::apply(
            &mut ui.state,
            GestureEvent {
                generation,
                stroke: 7,
                sequence,
                phase,
                point,
                output: Some("eDP-1".into()),
            },
        );
        ui.rebuild(&view);
        ui.dispatch(&view);
        ui.harness.render();
        assert_eq!(
            ui.state.outputs[0].drawing.generation, generation,
            "launch feedback revoked its own drawing"
        );
        assert_eq!(ui.state.outputs[0].drawing.bounds, Some(bounds));
        assert_eq!(
            popup::size(
                &ui.state.outputs[0],
                PopupKind::Search,
                f64::from(width),
                560.0
            )
            .height,
            height,
            "pending launch must not resize the compositor-armed popup"
        );
    }
    pending.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(ui.state.outputs[0].show_search);
    assert!(ui.state.outputs[0].launch_pending.is_some());
    assert!(has_text(&ui, "Opening application…"));
    release.send(()).unwrap();
    launch.complete(&mut ui.state);
    endpoint.join().unwrap();
    assert!(ui.state.outputs[0].launch_pending.is_none());
    assert_eq!(ui.state.outputs[0].show_search, !accepted);
    if !accepted {
        assert_eq!(
            ui.state.outputs[0].launch_error.as_deref(),
            Some("fixture launch failed")
        );
        ui.rebuild(&view);
        ui.dispatch(&view);
        assert!(has_text(
            &ui,
            "Could not complete the request: fixture launch failed"
        ));
        assert_eq!(ui.state.outputs[0].drawing.mode, Mode::Launch);
        ui.button("Back to apps list");
    }
}

fn has_text(ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>, text: &str) -> bool {
    crate::ui::test_support::ids(&ui.harness)
        .into_iter()
        .any(|id| {
            ui.harness.access_node(id).is_some_and(|node| {
                node.label().as_deref() == Some(text) || node.value().as_deref() == Some(text)
            })
        })
}
