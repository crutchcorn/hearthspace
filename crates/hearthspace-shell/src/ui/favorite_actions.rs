//! Drawing is an explicit search mode; configuration belongs to Settings.
use super::{command_client::send_command, favorite_state::Mode, state::ShellState};
use crate::favorites::{Point, Recognition};

pub(super) fn cancel(state: &mut ShellState, key: &str) {
    if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
        if output.drawing.armed {
            let _ = send_command(
                &state.command_socket,
                &format!("favorite-gesture-disarm {}", output.drawing.generation),
            );
        }
        output.drawing = Default::default();
    }
}

pub(super) fn cancel_all(state: &mut ShellState) {
    let keys: Vec<_> = state
        .outputs
        .iter()
        .filter(|o| o.drawing.active())
        .map(|o| o.key().to_owned())
        .collect();
    for key in keys {
        cancel(state, &key);
    }
}

pub(super) fn start(state: &mut ShellState, key: &str, mode: Mode) {
    cancel_all(state);
    if let Some(output) = state
        .outputs
        .iter_mut()
        .find(|output| output.key() == key && output.show_search)
    {
        output.drawing.start(mode);
        output.launch_error = None;
    }
}

/// Interpret a complete stroke; the compositor authorizes and launches atomically.
pub(super) fn complete(
    state: &mut ShellState,
    key: &str,
    points: Vec<Point>,
    generation: u64,
    stroke: u64,
) {
    let Some(output) = state
        .outputs
        .iter_mut()
        .find(|o| o.key() == key && o.show_search && o.drawing.mode == Mode::Launch)
    else {
        return;
    };
    let result = state.favorites.store.recognize(&points);
    output.drawing.preview = points;
    if let Ok(Recognition::Match(id)) = &result {
        launch(state, key, id, generation, stroke);
        return;
    }
    if let Err(error) = send_command(
        &state.command_socket,
        &format!("favorite-gesture-accept {generation} {stroke}"),
    ) {
        if let Some(output) = state.outputs.iter_mut().find(|o| o.key() == key) {
            output.drawing.preview.clear();
        }
        feedback(
            state,
            key,
            format!("Drawing expired or was cancelled: {error}"),
        );
        return;
    }
    match result {
        Ok(Recognition::Match(_)) => unreachable!(),
        Ok(Recognition::NoMatch) => feedback(
            state,
            key,
            "No matching gesture. Try again, or return to search.".into(),
        ),
        Ok(Recognition::Ambiguous) => feedback(
            state,
            key,
            "That shape could match more than one app. Try a clearer shape.".into(),
        ),
        Err(error) => feedback(state, key, error.to_string()),
    }
}

pub(super) fn manage(state: &mut ShellState, key: &str) {
    let name = state
        .outputs
        .iter()
        .find(|o| o.key() == key)
        .and_then(|o| o.output_name.clone());
    let result = super::command_client::launch_app(
        &state.command_socket,
        "dev.hearthspace.Settings.desktop",
        name.as_deref(),
    );
    state.finish_launch(key, result);
}

fn launch(state: &mut ShellState, key: &str, id: &str, generation: u64, stroke: u64) {
    if state.catalog.app_by_id(id).is_none() {
        feedback(
            state,
            key,
            "This favorite is no longer installed. Update it in Settings → Favorites.".into(),
        );
        return;
    }
    let result = send_command(
        &state.command_socket,
        &format!(
            "favorite-gesture-launch {generation} {stroke} {}",
            serde_json::to_string(id).expect("string is JSON serializable")
        ),
    );
    state.finish_launch(key, result);
}

fn feedback(state: &mut ShellState, key: &str, message: String) {
    if let Some(output) = state.outputs.iter_mut().find(|o| o.key() == key) {
        output.drawing.message = Some(message);
    }
}
