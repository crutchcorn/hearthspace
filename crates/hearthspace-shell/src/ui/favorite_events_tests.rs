use super::super::favorite_state::Mode;
use super::*;
use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    thread,
};

fn state(path: std::path::PathBuf) -> ShellState {
    let mut state = ShellState::new(path, vec!["eDP-1".into(), "HDMI-A-1".into()]);
    state.outputs[0].show_search = true;
    let drawing = &mut state.outputs[0].drawing;
    drawing.start(Mode::Launch);
    drawing.generation = 42;
    drawing.bounds = Some([10.0, 20.0, 400.0, 300.0]);
    drawing.armed = true;
    state
}

fn event(sequence: u64, phase: Phase, point: Point) -> GestureEvent {
    GestureEvent {
        generation: 42,
        stroke: 7,
        sequence,
        phase,
        point,
        output: Some("eDP-1".into()),
    }
}

fn draw(state: &mut ShellState) {
    apply(state, event(0, Phase::Begin, [20.0, 20.0]));
    apply(state, event(1, Phase::Update, [50.0, 130.0]));
    apply(state, event(2, Phase::Update, [180.0, 20.0]));
    apply(state, event(3, Phase::End, [180.0, 20.0]));
}

fn server(
    listener: UnixListener,
    expected: Vec<(&'static str, &'static str)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for (command, response) in expected {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).unwrap();
            assert_eq!(request, command);
            stream.write_all(response.as_bytes()).unwrap();
        }
    })
}

#[test]
fn complete_strokes_require_synchronous_compositor_acceptance() {
    for (response, accepted) in [("ok\n", true), ("err expired stroke\n", false)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("commands.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = server(listener, vec![("favorite-gesture-accept 42 7\n", response)]);
        let mut state = state(path);
        draw(&mut state);
        let drawing = &state.outputs[0].drawing;
        assert_eq!(!drawing.preview.is_empty(), accepted);
        assert!(drawing.incoming.is_none());
        if !accepted {
            assert!(drawing.message.as_deref().unwrap().contains("expired"));
        }
        server.join().unwrap();
    }
}

#[test]
fn gaps_duplicates_cancellation_and_out_of_bounds_never_reach_acceptance() {
    for corrupt in [
        event(2, Phase::Update, [50.0, 130.0]),
        event(0, Phase::Update, [50.0, 130.0]),
        event(1, Phase::Cancel, [20.0, 20.0]),
        event(1, Phase::Update, [400.0, 20.0]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("commands.sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut state = state(path);
        apply(&mut state, event(0, Phase::Begin, [20.0, 20.0]));
        apply(&mut state, corrupt);
        apply(&mut state, event(2, Phase::End, [180.0, 20.0]));
        assert!(state.outputs[0].drawing.incoming.is_none());
        assert!(state.outputs[0].drawing.preview.is_empty());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn stale_generation_wrong_output_closed_popup_and_unarmed_input_are_ignored() {
    let mut state = state("/nonexistent-favorite-test.sock".into());
    let mut stale = event(0, Phase::Begin, [20.0, 20.0]);
    stale.generation = 41;
    apply(&mut state, stale);
    let mut wrong_output = event(0, Phase::Begin, [20.0, 20.0]);
    wrong_output.output = Some("HDMI-A-1".into());
    apply(&mut state, wrong_output);
    assert!(state.outputs.iter().all(|o| o.drawing.incoming.is_none()));
    state.outputs[0].show_search = false;
    apply(&mut state, event(0, Phase::Begin, [20.0, 20.0]));
    state.outputs[0].show_search = true;
    state.outputs[0].drawing.armed = false;
    apply(&mut state, event(0, Phase::Begin, [20.0, 20.0]));
    assert!(state.outputs[0].drawing.incoming.is_none());
}

#[test]
fn bounded_points_and_missing_begin_fail_closed() {
    let mut state = state("/nonexistent-favorite-test.sock".into());
    apply(&mut state, event(1, Phase::End, [180.0, 20.0]));
    assert!(state.outputs[0].drawing.preview.is_empty());
    state.outputs[0].drawing.incoming = Some(Incoming {
        stroke: 7,
        next_sequence: 1,
        points: vec![[20.0, 20.0]; MAX_POINTS],
    });
    apply(&mut state, event(1, Phase::Update, [50.0, 130.0]));
    assert!(state.outputs[0].drawing.incoming.is_none());
    assert!(
        state.outputs[0]
            .drawing
            .message
            .as_deref()
            .unwrap()
            .contains("lost")
    );
}

#[test]
fn local_widget_input_never_bypasses_the_compositor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commands.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut state = state(path);
    local_stroke(
        &mut state,
        "eDP-1",
        42,
        vec![[20.0, 20.0], [50.0, 130.0], [180.0, 20.0]],
    );
    assert!(state.outputs[0].drawing.preview.is_empty());
    assert!(
        state.outputs[0]
            .drawing
            .message
            .as_deref()
            .unwrap()
            .contains("not captured")
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn initial_arm_retries_after_mapping_then_stops_after_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commands.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = server(
        listener,
        vec![
            (
                "favorite-gesture-arm 42 10 20 400 300\n",
                "err not mapped\n",
            ),
            ("favorite-gesture-arm 42 10 20 400 300\n", "ok\n"),
        ],
    );
    let mut state = state(path);
    state.outputs[0].drawing.armed = false;
    bounds(&mut state, "eDP-1", 42, [10.0, 20.0, 400.0, 300.0]);
    assert!(!state.outputs[0].drawing.armed);
    retry(&mut state);
    assert!(state.outputs[0].drawing.armed);
    retry(&mut state);
    assert_eq!(state.outputs[0].drawing.arm_attempts, 2);
    server.join().unwrap();
}

#[test]
fn expired_arming_retry_budget_stops_without_socket_access() {
    let mut state = state("/nonexistent-favorite-test.sock".into());
    let drawing = &mut state.outputs[0].drawing;
    drawing.armed = false;
    drawing.arm_attempts = 1;
    drawing.arm_retry_deadline = Some(Instant::now() - Duration::from_secs(1));
    retry(&mut state);
    assert_eq!(state.outputs[0].drawing.arm_attempts, 8);
    assert_eq!(
        state.outputs[0].drawing.message.as_deref(),
        Some(UNAVAILABLE)
    );
    assert!(!state.outputs[0].drawing.armed);
}

#[test]
fn parser_rejects_unbounded_or_incomplete_gesture_events() {
    let valid = serde_json::json!({"generation":42,"stroke":7,"sequence":0,
        "phase":"begin","x":20,"y":20,"output":"eDP-1"});
    assert_eq!(parse(&valid), Some(event(0, Phase::Begin, [20.0, 20.0])));
    for (key, value) in [
        ("generation", serde_json::json!(0)),
        ("stroke", serde_json::json!(0)),
        ("sequence", serde_json::json!(2050)),
        ("phase", serde_json::json!("launch")),
        ("x", serde_json::json!(-1)),
        ("y", serde_json::json!(1_000_001)),
        ("output", serde_json::json!(false)),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(parse(&invalid).is_none(), "{invalid}");
    }
}

#[test]
fn geometry_reconfiguration_discards_old_input_and_requires_new_layout_bounds() {
    let mut state = state("/nonexistent-favorite-test.sock".into());
    state.outputs[0].drawing.generation = u64::MAX;
    let mut begin = event(0, Phase::Begin, [20.0, 20.0]);
    begin.generation = u64::MAX;
    apply(&mut state, begin);
    let mut changed = event(0, Phase::Reconfigure, [0.0, 0.0]);
    changed.generation = u64::MAX;
    changed.stroke = 0;
    apply(&mut state, changed);
    let drawing = &state.outputs[0].drawing;
    assert!(drawing.incoming.is_none());
    assert!(!drawing.armed);
    assert_ne!(drawing.generation, u64::MAX);
    assert_eq!(drawing.bounds, None);
    apply(&mut state, event(1, Phase::End, [180.0, 20.0]));
    assert!(state.outputs[0].drawing.preview.is_empty());
}

#[test]
fn invalid_layout_bounds_disarm_capture_and_stop_automatic_retries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commands.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = server(listener, vec![("favorite-gesture-disarm 42\n", "ok\n")]);
    let mut state = state(path);
    bounds(&mut state, "eDP-1", 42, [10.0, 20.0, 0.0, 300.0]);
    assert!(!state.outputs[0].drawing.armed);
    assert_eq!(state.outputs[0].drawing.bounds, None);
    assert_eq!(state.outputs[0].drawing.arm_attempts, 8);
    retry(&mut state);
    server.join().unwrap();
}

#[test]
fn reconfiguration_parser_requires_the_exact_non_stroke_marker() {
    let value = serde_json::json!({"generation":42,"stroke":0,"sequence":0,
        "phase":"reconfigure","x":0,"y":0,"output":"eDP-1"});
    assert_eq!(parse(&value).unwrap().phase, Phase::Reconfigure);
    for field in ["stroke", "sequence", "x", "y"] {
        let mut invalid = value.clone();
        invalid[field] = serde_json::json!(1);
        assert!(parse(&invalid).is_none());
    }
}
