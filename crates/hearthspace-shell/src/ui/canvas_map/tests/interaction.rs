use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use masonry::core::{
    TextEvent,
    keyboard::{Key, NamedKey},
};

use super::*;

// Keep the real UI and command client; only the external compositor replies
// are substituted. Drain every request so extra reset/pan commands fail too.
fn commands(action: impl FnOnce(PathBuf)) -> Vec<String> {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("control.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let finished = done.clone();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut requests = Vec::new();
        while Instant::now() < deadline {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).unwrap();
                requests.push(request);
                stream.write_all(b"ok\n").unwrap();
            } else if finished.load(Ordering::Relaxed) {
                break;
            } else {
                thread::sleep(Duration::from_millis(2));
            }
        }
        requests
    });
    action(socket);
    done.store(true, Ordering::Relaxed);
    server.join().unwrap()
}

fn key<V: WidgetView<ShellState, Widget: Sized>>(
    ui: &mut Ui<V>,
    key: Key,
    build: &impl Fn(&ShellState) -> V,
) {
    ui.harness
        .process_text_event(TextEvent::key_down(key.clone()));
    ui.dispatch(build);
    ui.harness.process_text_event(TextEvent::key_up(key));
    ui.dispatch(build);
}

#[test]
fn reset_sends_only_output_zoom_to_and_live_status_disables_it_at_100_percent() {
    let sent = commands(|socket| {
        let mut state = state(0.85);
        state.command_socket = socket;
        let mut ui = Ui::new(state, panel_view, (352, 312));
        let reset = ui.button(RESET);
        let zoom = named(&ui, "Current canvas zoom");
        assert!(has_text(&ui, "85%"));
        assert!(!ui.harness.access_node(reset).unwrap().is_disabled());
        let before = ui.state.canvas.as_ref().unwrap().clone();
        ui.harness.mouse_click_on(reset, None);
        ui.dispatch(&panel_view);
        assert!(ui.state.canvas_error.is_none());
        assert!(ui.state.outputs[0].show_map);
        assert_eq!(ui.state.canvas.as_ref().unwrap(), &before);
        // Command acknowledgement must not manufacture a status update.
        assert!(has_text(&ui, "85%"));
        ui.state.canvas = Some(snapshot(1.0));
        ui.rebuild(&panel_view);
        ui.dispatch(&panel_view);
        assert_eq!(named(&ui, "Current canvas zoom"), zoom);
        assert!(has_text(&ui, "100%"));
        assert!(!has_text(&ui, "85%"));
        assert!(ui.harness.access_node(reset).unwrap().is_disabled());
        ui.harness.mouse_click_on(reset, None);
        ui.activate(reset);
        ui.dispatch(&panel_view);
        assert!(ui.state.canvas_error.is_none());
    });
    assert_eq!(sent, ["zoom-on-output 6544502d31 to 1\n"]);
}

#[test]
fn reset_at_100_percent_or_without_snapshot_cannot_send_a_command() {
    let sent = commands(|socket| {
        let mut state = state(1.0);
        state.command_socket = socket;
        let mut ui = Ui::new(state, panel_view, (352, 312));
        let reset = ui.button(RESET);
        // Include floating-point noise around the known 100% value.
        for snapshot in [Some(snapshot(1.0)), Some(snapshot(1.0 + 5e-7)), None] {
            ui.state.canvas = snapshot;
            ui.rebuild(&panel_view);
            ui.dispatch(&panel_view);
            assert!(ui.harness.access_node(reset).unwrap().is_disabled());
            ui.harness.mouse_click_on(reset, None);
            ui.activate(reset);
            ui.dispatch(&panel_view);
            assert!(ui.state.canvas_error.is_none());
            assert!(ui.state.outputs[0].show_map);
        }
        assert!(has_text(&ui, "—"));
        for name in ["Zoom out", "Zoom in"] {
            assert!(
                !ui.harness
                    .access_node(ui.button(name))
                    .unwrap()
                    .is_disabled()
            );
        }
    });
    assert!(sent.is_empty(), "disabled reset issued {sent:?}");
}

#[test]
fn keyboard_visits_reset_out_in_and_each_complete_key_sequence_sends_once() {
    let sent = commands(|socket| {
        let mut state = state(0.75);
        state.command_socket = socket;
        let mut ui = Ui::new(state, panel_view, (352, 312));
        ui.harness.focus_on(Some(ui.button(RESET)));
        key(&mut ui, Key::Named(NamedKey::Enter), &panel_view);
        ui.harness.press_tab_key(false);
        assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Zoom out")));
        key(&mut ui, Key::Character(" ".into()), &panel_view);
        ui.harness.press_tab_key(false);
        assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Zoom in")));
        key(&mut ui, Key::Named(NamedKey::Enter), &panel_view);
        assert!(ui.state.canvas_error.is_none());
        assert!(ui.state.outputs[0].show_map);
        ui.state.canvas = Some(snapshot(1.0));
        ui.rebuild(&panel_view);
        ui.dispatch(&panel_view);
        ui.harness.focus_on(Some(ui.button("Zoom in")));
        ui.harness.press_tab_key(true);
        assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Zoom out")));
        ui.harness.press_tab_key(true);
        assert_ne!(ui.harness.focused_widget_id(), Some(ui.button(RESET)));
    });
    assert_eq!(
        sent,
        [
            "zoom-on-output 6544502d31 to 1\n",
            "zoom-on-output 6544502d31 out\n",
            "zoom-on-output 6544502d31 in\n"
        ]
    );
}

#[test]
fn absent_snapshot_retains_working_zoom_directions_and_no_close_button() {
    let sent = commands(|socket| {
        let mut state = ShellState::new(socket, vec!["eDP-1".into()]);
        state.outputs[0].show_map = true;
        let mut ui = Ui::new(state, panel_view, (352, 312));
        assert!(!has_text(&ui, "Map unavailable · retrying…"));
        for name in ["Zoom out", "Zoom in"] {
            ui.activate(ui.button(name));
            ui.dispatch(&panel_view);
            assert!(ui.state.canvas_error.is_none());
            assert!(ui.state.outputs[0].show_map);
        }
        assert!(!ids(&ui.harness).into_iter().any(|id| {
            ui.harness.access_node(id).is_some_and(|node| {
                node.role() == Role::Button && node.label().as_deref() == Some("Close canvas map")
            })
        }));
    });
    assert_eq!(
        sent,
        [
            "zoom-on-output 6544502d31 out\n",
            "zoom-on-output 6544502d31 in\n"
        ]
    );
}

#[test]
fn secondary_map_controls_target_their_output_without_using_snapshot_zoom_steps() {
    let output = "Virtual output 二";
    let sent = commands(|socket| {
        let mut state = ShellState::new(socket, vec!["eDP-1".into(), output.into()]);
        state.outputs[1].show_map = true;
        state.canvas = Some(snapshot(0.75));
        // The snapshot can lag output discovery. Commands still target the
        // owning panel and let the compositor choose the current zoom step.
        let view = |state: &ShellState| {
            sized_box(panel(
                &state.outputs[1],
                state.canvas.as_ref(),
                state.canvas_error.as_deref(),
            ))
        };
        let mut ui = Ui::new(state, &view, (352, 312));
        for name in [RESET, "Zoom out", "Zoom in"] {
            ui.activate(ui.button(name));
            ui.dispatch(&view);
            assert!(ui.state.canvas_error.is_none());
            assert!(ui.state.outputs[1].show_map);
            assert!(!ui.state.outputs[0].show_map);
        }
    });
    let expected = [
        ZoomOperation::To(1.0),
        ZoomOperation::Out,
        ZoomOperation::In,
    ]
    .map(|operation| {
        Some(ShellCommand::ZoomOnOutput {
            output_name: output.into(),
            operation,
        })
    });
    assert_eq!(
        sent.iter()
            .map(|wire| ShellCommand::parse(wire))
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn map_without_an_output_cannot_issue_global_zoom_commands() {
    let sent = commands(|socket| {
        let mut state = ShellState::new(socket, vec![]);
        state.canvas = Some(snapshot(0.75));
        state.outputs[0].show_map = true;
        let mut ui = Ui::new(state, panel_view, (352, 312));
        for name in [RESET, "Zoom out", "Zoom in"] {
            let button = ui.button(name);
            assert!(ui.harness.access_node(button).unwrap().is_disabled());
            ui.activate(button);
            ui.dispatch(&panel_view);
        }
    });
    assert!(sent.is_empty(), "unbound map issued {sent:?}");
}
