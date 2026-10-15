//! Refresh map geometry only while a map is open. No capture/readback is needed.
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

type CanvasResult = Result<Option<CanvasSnapshot>, String>;
type CanvasRefresh = (u64, CanvasResult);

use super::{
    command_client::query_canvas,
    minimap::{CanvasSnapshot, parse_snapshot},
    state::ShellState,
};

pub(super) fn canvas_task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<CanvasRefresh>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            let revision = state.canvas_revision.clone();
            async move {
                loop {
                    let socket = socket.clone();
                    let requested_revision = revision.load(std::sync::atomic::Ordering::Acquire);
                    let result = xilem::tokio::task::spawn_blocking(move || {
                        let bytes = query_canvas(&socket)?;
                        let status: serde_json::Value = serde_json::from_slice(&bytes)
                            .map_err(|error| format!("Invalid canvas status: {error}"))?;
                        if status["locked"] == true || status["active"] == false {
                            return Ok(None);
                        }
                        parse_snapshot(&bytes).map(Some)
                    })
                    .await
                    .unwrap_or_else(|error| Err(format!("Canvas query failed: {error}")));
                    // Deliver even unchanged snapshots: a drag can have
                    // deferred the prior result, or a command error may need
                    // clearing after the connection recovers.
                    if proxy.message((requested_revision, result)).is_err() {
                        break;
                    }
                    xilem::tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        },
        apply_refresh,
    )
}

fn apply_refresh(state: &mut ShellState, (revision, result): CanvasRefresh) {
    // The compositor may have answered a query before a final pan-to command,
    // while its result reaches this event loop after pointer release. Reject
    // those older snapshots instead of snapping the handle back for one frame.
    if revision
        == state
            .canvas_revision
            .load(std::sync::atomic::Ordering::Acquire)
        || matches!(result, Ok(None))
    {
        apply_canvas(state, result);
    }
}

fn apply_canvas(state: &mut ShellState, result: Result<Option<CanvasSnapshot>, String>) {
    match result {
        Ok(snapshot) => {
            if snapshot.is_none() {
                state.canvas_dragging = false;
                for output in &mut state.outputs {
                    output.show_map = false;
                }
            } else if state.canvas_dragging {
                // The captured drag owns the viewport until pointer release.
                // A polling result must not replace its optimistic offset.
                return;
            }
            state.canvas = snapshot;
            state.canvas_error = None;
        }
        // Keep the last confirmed map through transient IPC delays, but make
        // its stale state explicit in the popover.
        Err(error) => state.canvas_error = Some(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(scale: f64) -> CanvasSnapshot {
        let value = serde_json::json!({
            "viewport": {"x": 0, "y": 0, "scale": scale},
            "outputs": [{"name": "eDP-1", "logical_x": 0, "logical_y": 0,
                         "logical_width": 1280, "logical_height": 800}],
            "windows": [],
        });
        parse_snapshot(&serde_json::to_vec(&value).unwrap()).unwrap()
    }

    #[test]
    fn refresh_waits_for_drag_release_and_recovers_after_transport_errors() {
        let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
        state.canvas = Some(snapshot(1.0));
        state.canvas_dragging = true;
        apply_canvas(&mut state, Ok(Some(snapshot(0.8))));
        assert_eq!(state.canvas.as_ref().unwrap().scale(), 1.0);
        state.canvas_dragging = false;
        apply_canvas(&mut state, Err("timeout".into()));
        assert_eq!(state.canvas.as_ref().unwrap().scale(), 1.0);
        assert!(state.canvas_error.is_some());
        apply_canvas(&mut state, Ok(Some(snapshot(0.8))));
        assert_eq!(state.canvas.as_ref().unwrap().scale(), 0.8);
        assert!(state.canvas_error.is_none());
    }

    #[test]
    fn locked_snapshot_clears_private_map_and_captured_drag() {
        let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
        state.toggle_map("eDP-1");
        state.canvas = Some(snapshot(1.0));
        state.canvas_dragging = true;
        apply_canvas(&mut state, Ok(None));
        assert!(state.canvas.is_none());
        assert!(!state.canvas_dragging);
        assert!(!state.outputs[0].show_map);
    }

    #[test]
    fn delayed_query_from_before_drag_release_cannot_replace_final_view() {
        let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
        let mut final_view = snapshot(1.0);
        final_view.set_offset(300, -200);
        state.canvas = Some(final_view.clone());
        state
            .canvas_revision
            .store(1, std::sync::atomic::Ordering::Release);
        apply_refresh(&mut state, (0, Ok(Some(snapshot(1.0)))));
        assert_eq!(state.canvas, Some(final_view));
        let latest = snapshot(0.8);
        apply_refresh(&mut state, (1, Ok(Some(latest.clone()))));
        assert_eq!(state.canvas, Some(latest));
        apply_refresh(&mut state, (0, Ok(None)));
        assert!(
            state.canvas.is_none(),
            "locking must clear even a newer map"
        );
    }
}
