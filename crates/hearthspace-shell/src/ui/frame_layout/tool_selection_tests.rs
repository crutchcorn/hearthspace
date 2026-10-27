use super::*;
use crate::ui::{
    frame_layout,
    quick_panel::Kind,
    test_support::{Ui, ids},
};
use masonry::accesskit::{Live, Role, Toggled};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    thread,
};
use xilem::{WidgetView, view::sized_box};

fn fixture() -> (ShellState, tokio::sync::mpsc::Receiver<Request>) {
    let mut state = ShellState::new("/unused".into(), vec!["test".into(), "other".into()]);
    apply_update(
        &mut state,
        Update::Snapshot(Box::new(Snapshot {
            version: 1,
            canvas_tool: Some(CanvasTool::Select),
            outputs: vec![frame_layout::Output {
                output: "test".into(),
                ..Default::default()
            }],
            ..Default::default()
        })),
    );
    state.outputs[0].quick_panel = Kind::Tools;
    state.outputs[0].show_controls = true;
    let (sender, requests) = tokio::sync::mpsc::channel(1);
    state.frame_layout.requests = Some(sender);
    (state, requests)
}

fn menu(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(frame_layout::tool_menu::panel(state, &state.outputs[0]))
}

fn feedback<V: WidgetView<ShellState, Widget: Sized>>(ui: &Ui<V>) {
    let node = ids(&ui.harness)
        .into_iter()
        .find_map(|id| {
            ui.harness
                .access_node(id)
                .filter(|node| node.role() == Role::Status)
        })
        .expect("the chooser must expose its failure as status feedback");
    assert_eq!(node.live(), Live::Polite);
    assert!(node.label().unwrap().contains("Choose it again to retry"));
    let bounds = node.bounding_box().unwrap();
    assert!(
        bounds.x0 >= 0.0 && bounds.x1 <= 260.0 && bounds.y0 >= 0.0 && bounds.y1 <= 208.0,
        "the error must be visible beside its retry choices: {bounds:?}"
    );
    assert!(ui.state.outputs[0].show_controls);
    assert_eq!(
        ui.harness
            .access_node(ui.button("Pan canvas"))
            .unwrap()
            .toggled(),
        Some(Toggled::False)
    );
}

#[test]
fn rejected_selection_renders_feedback_then_an_explicit_retry_waits_for_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("control.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let snapshot = serde_json::to_vec(&Snapshot {
        version: 1,
        canvas_tool: Some(CanvasTool::Pan),
        outputs: vec![frame_layout::Output {
            output: "test".into(),
            ..Default::default()
        }],
        ..Default::default()
    })
    .unwrap();
    let server = thread::spawn(move || {
        let mut commands = Vec::new();
        for reply in [
            b"err rejected\n".to_vec(),
            b"ok\n".to_vec(),
            [format!("ok {}\n", snapshot.len()).into_bytes(), snapshot].concat(),
        ] {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing worker request");
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut command = String::new();
            BufReader::new(&mut stream).read_line(&mut command).unwrap();
            commands.push(command);
            stream.write_all(&reply).unwrap();
        }
        commands
    });
    let (mut state, mut requests) = fixture();
    state.command_socket = path.clone();
    let mut ui = Ui::new(state, menu, (260, 208));
    let before = ui.harness.render();
    let pan = ui.button("Pan canvas");
    ui.activate(pan);
    ui.dispatch(&menu);
    let request = requests.try_recv().unwrap();
    let selection = request.selection.clone().unwrap();
    assert!(ui.state.outputs[0].show_controls);
    assert!(ui.state.outputs[0].tool_error.is_none());
    let mut worker = Worker::default();
    let error = worker.poll(&path, Some(request), vec![], 1).unwrap_err();
    assert!(
        ui.state.outputs[0].tool_error.is_none(),
        "worker completion awaits UI delivery"
    );
    apply_update(&mut ui.state, Update::Error(error.clone()));
    apply_update(&mut ui.state, Update::Selection(selection, Err(error)));
    ui.rebuild(&menu);
    ui.dispatch(&menu);
    feedback(&ui);
    assert_ne!(
        before,
        ui.harness.render(),
        "failure must alter the rendered chooser"
    );
    assert_eq!(ui.button("Pan canvas"), pan);
    ui.activate(pan);
    ui.dispatch(&menu);
    assert!(ui.state.outputs[0].tool_error.is_none());
    assert!(ui.state.outputs[0].show_controls);
    let request = requests.try_recv().unwrap();
    let selection = request.selection.clone().unwrap();
    let snapshot = worker.poll(&path, Some(request), vec![], 2).unwrap();
    assert_ne!(
        ui.state.frame_layout.snapshot.canvas_tool,
        Some(CanvasTool::Pan)
    );
    apply_update(&mut ui.state, Update::Snapshot(Box::new(snapshot)));
    apply_update(&mut ui.state, Update::Selection(selection, Ok(())));
    ui.rebuild(&menu);
    ui.dispatch(&menu);
    assert!(!ui.state.outputs[0].show_controls);
    assert_eq!(
        ui.harness.access_node(pan).unwrap().toggled(),
        Some(Toggled::True)
    );
    assert_eq!(
        server.join().unwrap(),
        ["canvas-tool pan\n", "canvas-tool pan\n", "layout-status\n"]
    );
}

#[test]
fn unavailable_closed_and_full_queues_leave_visible_retry_feedback() {
    for failure in ["unavailable", "closed", "full"] {
        let (mut state, requests) = fixture();
        let mut requests = Some(requests);
        match failure {
            "unavailable" => state.frame_layout.requests = None,
            "closed" => drop(requests.take()),
            "full" => frame_layout::submit(&mut state, "layout-status".into()),
            _ => unreachable!(),
        }
        let mut ui = Ui::new(state, menu, (260, 208));
        ui.activate(ui.button("Pan canvas"));
        ui.dispatch(&menu);
        feedback(&ui);
        assert_eq!(
            ui.state.frame_layout.snapshot.canvas_tool,
            Some(CanvasTool::Select)
        );
        assert_eq!(
            ui.harness
                .access_node(ui.button("Select desktop items"))
                .unwrap()
                .toggled(),
            Some(Toggled::True)
        );
        if failure == "full" {
            assert_eq!(
                requests.as_mut().unwrap().try_recv().unwrap().command,
                "layout-status"
            );
        }
    }
}

#[test]
fn late_failures_cannot_reopen_dismissed_choosers_or_replace_newer_output_attempts() {
    let (mut state, mut requests) = fixture();
    frame_layout::tool_selection::choose(&mut state, "test", CanvasTool::Pan);
    let old = requests.try_recv().unwrap().selection.unwrap();
    state.dismiss_controls("test");
    old.complete(&mut state, Err("late failure".into()));
    assert!(!state.outputs[0].show_controls);
    assert!(state.outputs[0].tool_error.is_none());

    state.outputs[0].show_controls = true;
    frame_layout::tool_selection::choose(&mut state, "test", CanvasTool::Pan);
    let old = requests.try_recv().unwrap().selection.unwrap();
    frame_layout::tool_selection::choose(&mut state, "test", CanvasTool::Search);
    let current = requests.try_recv().unwrap().selection.unwrap();
    old.complete(&mut state, Err("superseded".into()));
    assert!(state.outputs[0].show_controls && state.outputs[0].tool_error.is_none());
    frame_layout::popups::open(
        &mut state,
        "other",
        frame_layout::popups::Popup::Quick(Kind::Tools),
    );
    current
        .clone()
        .complete(&mut state, Err("wrong output".into()));
    state.frame_layout.snapshot.canvas_tool = Some(CanvasTool::Search);
    current.complete(&mut state, Ok(()));
    assert!(!state.outputs[0].show_controls);
    assert!(state.outputs[1].show_controls);
    assert!(
        state
            .outputs
            .iter()
            .all(|output| output.tool_error.is_none())
    );
}
