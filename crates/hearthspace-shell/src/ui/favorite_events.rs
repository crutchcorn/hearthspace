//! Ordered, bounded compositor strokes. A local widget callback is never enough
//! to launch: completed input must still be authorized by the active compositor.
use std::time::{Duration, Instant};

use crate::favorites::Point;

use super::{command_client::send_command, favorite_state::Incoming, state::ShellState};

const MAX_POINTS: usize = 2050;
const UNAVAILABLE: &str = "Drawing is unavailable. Go back and reopen drawing mode.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Begin,
    Update,
    End,
    Cancel,
    Reconfigure,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct GestureEvent {
    pub generation: u64,
    pub stroke: u64,
    pub sequence: u64,
    pub phase: Phase,
    pub point: Point,
    pub output: Option<String>,
}

pub(super) fn parse(value: &serde_json::Value) -> Option<GestureEvent> {
    let phase = match value["phase"].as_str()? {
        "begin" => Phase::Begin,
        "update" => Phase::Update,
        "end" => Phase::End,
        "cancel" => Phase::Cancel,
        "reconfigure" => Phase::Reconfigure,
        _ => return None,
    };
    let generation = value["generation"].as_u64().filter(|v| *v > 0)?;
    let stroke = value["stroke"].as_u64()?;
    let sequence = value["sequence"]
        .as_u64()
        .filter(|v| *v < MAX_POINTS as u64)?;
    let point = [value["x"].as_f64()?, value["y"].as_f64()?];
    if point
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0 || *v > 1_000_000.0)
    {
        return None;
    }
    if phase == Phase::Reconfigure {
        if stroke != 0 || sequence != 0 || point != [0.0, 0.0] {
            return None;
        }
    } else if stroke == 0 {
        return None;
    }
    let output = match value.get("output")? {
        serde_json::Value::Null => None,
        serde_json::Value::String(output) if !output.is_empty() && output.len() <= 256 => {
            Some(output.clone())
        }
        _ => return None,
    };
    Some(GestureEvent {
        generation,
        stroke,
        sequence,
        phase,
        point,
        output,
    })
}

pub(super) fn bounds(state: &mut ShellState, key: &str, generation: u64, rect: [f64; 4]) {
    let Some(output) = state.outputs.iter_mut().find(|o| o.key() == key) else {
        return;
    };
    if !output.show_search || !output.drawing.active() || output.drawing.generation != generation {
        return;
    }
    if rect
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0 || *v > 1_000_000.0)
        || rect[2] < 16.0
        || rect[3] < 16.0
    {
        if output.drawing.armed {
            let _ = send_command(
                &state.command_socket,
                &format!("favorite-gesture-disarm {generation}"),
            );
        }
        output.drawing.armed = false;
        output.drawing.bounds = None;
        output.drawing.arm_attempts = 8;
        output.drawing.arm_retry_deadline = None;
        output.drawing.incoming = None;
        output.drawing.preview.clear();
        output.drawing.message = Some("The drawing area is unavailable.".into());
        return;
    }
    if output.drawing.bounds == Some(rect) && output.drawing.armed {
        return;
    }
    if output.drawing.bounds.is_some_and(|old| old != rect) {
        if output.drawing.armed {
            let _ = send_command(
                &state.command_socket,
                &format!("favorite-gesture-disarm {generation}"),
            );
        }
        output.drawing.refresh_generation();
    }
    output.drawing.bounds = Some(rect);
    output.drawing.incoming = None;
    output.drawing.preview.clear();
    let now = Instant::now();
    let deadline = *output
        .drawing
        .arm_retry_deadline
        .get_or_insert(now + Duration::from_secs(2));
    if output.drawing.arm_attempts >= 8 || now >= deadline {
        output.drawing.arm_attempts = 8;
        output.drawing.message = Some(UNAVAILABLE.into());
        return;
    }
    output.drawing.arm_attempts += 1;
    let generation = output.drawing.generation;
    let result = send_command(
        &state.command_socket,
        &format!(
            "favorite-gesture-arm {generation} {} {} {} {}",
            rect[0], rect[1], rect[2], rect[3]
        ),
    );
    output.drawing.armed = result.is_ok();
    output.drawing.message = result.err().map(|_| {
        if output.drawing.arm_attempts >= 8 || Instant::now() >= deadline {
            UNAVAILABLE.into()
        } else {
            "Preparing drawing area...".into()
        }
    });
}

pub(super) fn retry(state: &mut ShellState) {
    let now = Instant::now();
    for output in &mut state.outputs {
        let drawing = &mut output.drawing;
        if output.show_search
            && drawing.active()
            && !drawing.armed
            && drawing.arm_attempts < 8
            && drawing
                .arm_retry_deadline
                .is_some_and(|deadline| now >= deadline)
        {
            drawing.arm_attempts = 8;
            drawing.message = Some(UNAVAILABLE.into());
        }
    }
    let pending: Vec<_> = state
        .outputs
        .iter()
        .filter_map(|output| {
            let drawing = &output.drawing;
            (output.show_search
                && drawing.active()
                && !drawing.armed
                && drawing.arm_attempts < 8
                && drawing
                    .arm_retry_deadline
                    .is_some_and(|deadline| now < deadline))
            .then_some(drawing.bounds)
            .flatten()
            .map(|rect| (output.key().to_owned(), drawing.generation, rect))
        })
        .collect();
    for (key, generation, rect) in pending {
        bounds(state, &key, generation, rect);
    }
}

pub(super) fn reload(state: &mut ShellState) {
    if state.favorites.refresh() {
        if state.favorites.tracks_installed_apps {
            state.catalog = crate::app_catalog::AppCatalog::load();
        }
        // Never recognize an in-flight shape against an assignment that changed
        // after the user started drawing it.
        super::favorite_actions::cancel_all(state);
    }
}

pub(super) fn local_stroke(state: &mut ShellState, key: &str, generation: u64, _: Vec<Point>) {
    if let Some(output) = state.outputs.iter_mut().find(|o| o.key() == key)
        && output.show_search
        && output.drawing.active()
        && output.drawing.generation == generation
    {
        output.drawing.message =
            Some("Drawing input was not captured. Reopen drawing mode and try again.".into());
    }
}

pub(super) fn apply(state: &mut ShellState, event: GestureEvent) {
    let Some(output) = state
        .outputs
        .iter_mut()
        .find(|o| o.output_name == event.output)
    else {
        return;
    };
    let drawing = &mut output.drawing;
    if !output.show_search
        || !drawing.active()
        || !drawing.armed
        || drawing.generation != event.generation
    {
        return;
    }
    if event.phase == Phase::Reconfigure {
        drawing.refresh_generation();
        drawing.bounds = None;
        drawing.preview.clear();
        drawing.message = Some("The drawing area changed. Draw the shape again.".into());
        return;
    }
    let inside = drawing.bounds.is_some_and(|b| {
        event.point[0] >= 0.0
            && event.point[1] >= 0.0
            && event.point[0] < b[2]
            && event.point[1] < b[3]
    });
    if !inside || event.sequence >= MAX_POINTS as u64 {
        drawing.incoming = None;
        drawing.preview.clear();
        drawing.message = Some("The stroke left the drawing area. Try again.".into());
        return;
    }
    if event.phase == Phase::Cancel {
        if drawing
            .incoming
            .as_ref()
            .is_some_and(|i| i.stroke == event.stroke)
        {
            drawing.incoming = None;
            drawing.preview.clear();
            drawing.message =
                Some("Drawing cancelled. Draw again with one finger or one pointer.".into());
        }
        return;
    }
    if event.phase == Phase::Begin {
        if event.sequence != 0 || drawing.incoming.is_some() {
            drawing.incoming = None;
            drawing.preview.clear();
            drawing.message = Some("Drawing was interrupted. Start the shape again.".into());
            return;
        }
        drawing.incoming = Some(Incoming {
            stroke: event.stroke,
            next_sequence: 1,
            points: vec![event.point],
        });
        drawing.preview = vec![event.point];
        drawing.message = None;
        return;
    }
    let Some(incoming) = drawing.incoming.as_mut() else {
        return;
    };
    if incoming.stroke != event.stroke
        || incoming.next_sequence != event.sequence
        || incoming.points.len() >= MAX_POINTS
    {
        drawing.incoming = None;
        drawing.preview.clear();
        drawing.message = Some("Part of the stroke was lost. Draw the shape again.".into());
        return;
    }
    incoming.next_sequence += 1;
    incoming.points.push(event.point);
    drawing.preview.clone_from(&incoming.points);
    if event.phase != Phase::End {
        return;
    }
    let points = drawing.incoming.take().unwrap().points;
    let key = output.key().to_owned();
    super::favorite_actions::complete(state, &key, points, event.generation, event.stroke);
}

#[cfg(test)]
#[path = "favorite_events_tests.rs"]
mod tests;
