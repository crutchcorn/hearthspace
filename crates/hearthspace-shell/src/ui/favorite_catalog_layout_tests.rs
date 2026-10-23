//! Mount the real WindowView root without a native window lifecycle. Each
//! observation builds independently, then feeds its measured pad rectangle
//! through the production bounds callback. Compositor arm authorization is
//! supplied at the boundary, as in the retained favorite UI tests.
use std::sync::Arc;

use masonry::{core::Widget, testing::TestHarness, widgets::SizedBox};
use xilem::{
    ViewCtx,
    core::{RawProxy, View},
    dpi::Size,
};

use super::{ShellState, launcher, launcher_window};
use crate::ui::{
    favorite_events::{self, GestureEvent, Phase},
    favorite_state::Mode,
    test_support::ids,
};

#[derive(Debug)]
struct NoMessages;

impl RawProxy for NoMessages {
    fn send_message(
        &self,
        _: Arc<[xilem::core::ViewId]>,
        _: xilem::core::SendMessage,
    ) -> Result<(), xilem::core::ProxyError> {
        panic!("launcher content must not start background tasks")
    }

    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

struct Observation {
    creation_sizes: [Option<Size>; 3],
    pad: Option<[f64; 4]>,
    content: [f64; 4],
    text: Vec<String>,
}

impl Observation {
    fn has_text(&self, expected: &str) -> bool {
        self.text.iter().any(|text| text == expected)
    }
}

fn observe(state: &mut ShellState) -> Observation {
    let output = &state.outputs[0];
    let view = launcher_window(
        state,
        output,
        launcher::results(state, output),
        state
            .windows_error
            .clone()
            .or_else(|| state.desktop.source_error.clone()),
    );
    let runtime = xilem::tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut ctx = ViewCtx::new(Arc::new(NoMessages), Arc::new(runtime));
    let (window, _) = view.build(&mut ctx, state);
    let attrs = window.0.attributes;
    let creation_sizes = [attrs.inner_size, attrs.min_inner_size, attrs.max_inner_size];
    // Keep the available layout area identical across observations. Creation
    // hints are checked separately, before resize-observer measurements exist.
    let mut harness = TestHarness::create_with_size(
        crate::ui::appearance::properties(state.appearance.palette),
        SizedBox::new(window.0.root_widget).prepare(),
        (650, 560),
    );
    harness.render();
    let mut text = Vec::new();
    let mut pad = None;
    let mut content = None;
    for id in ids(&harness) {
        let Some(node) = harness.access_node(id) else {
            continue;
        };
        let name = node.label();
        text.extend(name.iter().cloned());
        text.extend(node.value());
        if matches!(
            name.as_deref(),
            Some("Gesture drawing area" | "Apps and windows")
        ) {
            let widget = harness.get_widget_with_id(id);
            let rect = widget
                .ctx()
                .window_transform()
                .transform_rect_bbox(widget.ctx().border_box());
            let rect = [rect.x0, rect.y0, rect.width(), rect.height()];
            if name.as_deref() == Some("Gesture drawing area") {
                pad = Some(rect);
            } else {
                content = Some(rect);
            }
        }
    }
    Observation {
        creation_sizes,
        pad,
        content: content.expect("real launcher content group"),
        text,
    }
}

fn state(drawing: bool) -> ShellState {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.catalog = Default::default();
    state.outputs[0].show_search = true;
    if drawing {
        state.outputs[0].drawing.start(Mode::Launch);
    }
    state
}

#[derive(Clone, Copy, Debug)]
enum Catalog {
    Windows,
    Desktop,
}

impl Catalog {
    fn error(self) -> &'static str {
        match self {
            Self::Windows => "Window list unavailable: Resource temporarily unavailable",
            Self::Desktop => "Desktop catalog temporarily unavailable",
        }
    }

    fn set(self, state: &mut ShellState, present: bool) {
        let error = present.then(|| self.error().to_owned());
        match self {
            Self::Windows => state.windows_error = error,
            Self::Desktop => state.desktop.source_error = error,
        }
    }
}

fn event(generation: u64, sequence: u64, phase: Phase, point: [f64; 2]) -> GestureEvent {
    GestureEvent {
        generation,
        stroke: 7,
        sequence,
        phase,
        point,
        output: Some("eDP-1".into()),
    }
}

fn assert_catalog_does_not_interrupt(catalog: Catalog) {
    let mut state = state(true);
    let ready = observe(&mut state);
    let pad = ready.pad.expect("drawing pad");
    let drawing = &mut state.outputs[0].drawing;
    drawing.bounds = Some(pad);
    drawing.armed = true;
    let generation = drawing.generation;
    let mut points = vec![[30.0, 30.0], [30.0, 60.0]];
    favorite_events::apply(&mut state, event(generation, 0, Phase::Begin, points[0]));
    favorite_events::apply(&mut state, event(generation, 1, Phase::Update, points[1]));
    let before = observe(&mut state);
    assert_eq!(before.pad, Some(pad));
    for (sequence, present) in [(2, true), (3, false)] {
        catalog.set(&mut state, present);
        let after = observe(&mut state);
        favorite_events::bounds(
            &mut state,
            "eDP-1",
            generation,
            after.pad.expect("drawing pad survives catalog update"),
        );
        assert_eq!(
            after.pad, before.pad,
            "{catalog:?}, error present={present}"
        );
        assert_eq!(
            after.content, before.content,
            "drawing content must not resize"
        );
        assert_eq!(
            after.creation_sizes, before.creation_sizes,
            "native sizing must not change"
        );
        assert!(!after.has_text(&format!(
            "Could not complete the request: {}",
            catalog.error()
        )));
        let drawing = &state.outputs[0].drawing;
        assert!(drawing.armed);
        assert_eq!(drawing.generation, generation);
        assert_eq!(drawing.bounds, Some(pad));
        let incoming = drawing
            .incoming
            .as_ref()
            .expect("in-flight stroke survives");
        assert_eq!(incoming.stroke, 7);
        assert_eq!(incoming.next_sequence, sequence);
        assert_eq!(incoming.points, points);
        assert_eq!(drawing.preview, points);
        // The original generation must still accept the next real input event.
        let point = [30.0 + sequence as f64 * 10.0, 60.0];
        favorite_events::apply(
            &mut state,
            event(generation, sequence, Phase::Update, point),
        );
        points.push(point);
        assert_eq!(
            state.outputs[0].drawing.incoming.as_ref().unwrap().points,
            points
        );
    }
}

#[test]
fn window_catalog_error_does_not_move_or_cancel_active_drawing() {
    assert_catalog_does_not_interrupt(Catalog::Windows);
}

#[test]
fn desktop_catalog_error_does_not_move_or_cancel_active_drawing() {
    assert_catalog_does_not_interrupt(Catalog::Desktop);
}

#[test]
fn catalog_errors_remain_visible_while_browsing() {
    for catalog in [Catalog::Windows, Catalog::Desktop] {
        let mut state = state(false);
        catalog.set(&mut state, true);
        let message = format!("Could not complete the request: {}", catalog.error());
        assert!(observe(&mut state).has_text(&message), "{catalog:?}");
        catalog.set(&mut state, false);
        assert!(!observe(&mut state).has_text(&message), "{catalog:?}");
    }
}

#[test]
fn genuine_launch_and_drawing_errors_remain_visible() {
    let mut state = state(true);
    Catalog::Windows.set(&mut state, true);
    state.outputs[0].launch_error = Some("Favorite application could not start".into());
    let failed_launch = observe(&mut state);
    assert!(
        failed_launch
            .has_text("Could not complete the request: Favorite application could not start")
    );
    state.outputs[0].launch_error = None;
    state.outputs[0].drawing.message =
        Some("No matching gesture. Try again, or return to Apps List.".into());
    let failed_stroke = observe(&mut state);
    assert!(failed_stroke.has_text("No matching gesture. Try again, or return to Apps List."));
    assert!(failed_stroke.has_text("Clear drawing and retry"));
}
