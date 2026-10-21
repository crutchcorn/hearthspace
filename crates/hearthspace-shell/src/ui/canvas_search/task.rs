//! Coalesced text updates and blocking IPC never run on Masonry's event thread.
use super::{ShellState, Snapshot, apply_snapshot, controls};
use crate::ui::{command_client, control_worker};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

#[derive(Clone, Debug, Serialize)]
pub(super) struct Edit {
    pub session: u64,
    pub query: String,
    pub handwriting: bool,
}

#[derive(Debug)]
enum Update {
    Snapshot(Snapshot),
    Action(String, Option<String>),
    Error(String),
    Unavailable(String),
}

pub(in crate::ui) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<Update>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            let refresh = state.canvas_search.refresh.clone();
            let (sender, mut requests) = tokio::sync::mpsc::channel::<String>(32);
            let (edits, mut changes) = tokio::sync::watch::channel::<Option<Edit>>(None);
            state.canvas_search.requests = Some(sender);
            state.canvas_search.edits = Some(edits);
            async move {
                let mut registered = false;
                let mut retry_at = tokio::time::Instant::now();
                let mut interval = tokio::time::interval(Duration::from_millis(500));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut previous = None;
                let mut unavailable = false;
                loop {
                    let command = tokio::select! {
                        biased;
                        changed = changes.changed() => {
                            if changed.is_err() { break; }
                            changes.borrow_and_update().clone().map(|edit| format!("canvas-search-update {}", serde_json::to_string(&edit).unwrap()))
                        }
                        request = requests.recv() => {
                            let Some(request) = request else { break; };
                            Some(request)
                        }
                        _ = refresh.notified() => None,
                        _ = interval.tick() => None,
                    };
                    if !registered && tokio::time::Instant::now() >= retry_at {
                        registered = register(socket.clone()).await;
                        retry_at = tokio::time::Instant::now() + Duration::from_secs(2);
                    }
                    if let Some(command) = command {
                        let path = socket.clone();
                        let accepted_action = command.starts_with("canvas-search-event-accept ");
                        let result = tokio::task::spawn_blocking(move || {
                            if accepted_action {
                                #[derive(Deserialize)]
                                struct Accepted {
                                    id: String,
                                    output: Option<String>,
                                }
                                let bytes = command_client::query_payload(
                                    &path,
                                    &command,
                                    Duration::from_millis(750),
                                )
                                .map_err(|e| e.to_string())?;
                                let value: Accepted =
                                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                                Ok(Some(Update::Action(value.id, value.output)))
                            } else {
                                command_client::send_command(&path, &command).map(|()| None)
                            }
                        })
                        .await;
                        match result {
                            Ok(Ok(Some(update)))
                                if control_worker::session(socket.clone()).await.available =>
                            {
                                if proxy.message(update).is_err() {
                                    break;
                                }
                            }
                            Ok(Err(error)) if !accepted_action => {
                                previous = None;
                                if proxy.message(Update::Error(error)).is_err() {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let path = socket.clone();
                    let snapshot = tokio::task::spawn_blocking(move || {
                        command_client::query_canvas_search(&path)
                            .map_err(|error| error.to_string())
                            .and_then(|bytes| parse(&bytes))
                    })
                    .await;
                    match snapshot {
                        Ok(Ok(snapshot)) => {
                            unavailable = false;
                            if previous.as_ref() != Some(&snapshot) {
                                previous = Some(snapshot.clone());
                                if proxy.message(Update::Snapshot(snapshot)).is_err() {
                                    break;
                                }
                            }
                        }
                        result => {
                            registered = false;
                            previous = None;
                            if !unavailable {
                                unavailable = true;
                                let error = match result {
                                    Ok(Err(error)) => error,
                                    Err(error) => error.to_string(),
                                    _ => unreachable!(),
                                };
                                if proxy.message(Update::Unavailable(error)).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        },
        apply,
    )
}

async fn register(socket: PathBuf) -> bool {
    tokio::task::spawn_blocking(move || {
        for (index, entries) in controls::registration().chunks(12).enumerate() {
            let command = format!(
                "canvas-search-register {}",
                serde_json::json!({"offset":index * 12,"entries":entries})
            );
            if command_client::send_command(&socket, &command).is_err() {
                return false;
            }
        }
        true
    })
    .await
    .unwrap_or(false)
}

fn apply(state: &mut ShellState, update: Update) {
    match update {
        Update::Snapshot(snapshot) => apply_snapshot(state, snapshot),
        Update::Action(id, output) => controls::activate(state, &id, output.as_deref()),
        Update::Error(error) => state.canvas_search.error = Some(error),
        Update::Unavailable(error) => {
            apply_snapshot(state, Snapshot::default());
            state.canvas_search.error = Some(error);
        }
    }
}

fn parse(bytes: &[u8]) -> Result<Snapshot, String> {
    let snapshot: Snapshot = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid canvas search state: {error}"))?;
    let field = &snapshot.field;
    if snapshot.version != 1
        || snapshot.results.len() > 8192
        || [
            field.x,
            field.y,
            field.width,
            field.height,
            snapshot.handwriting_height,
        ]
        .iter()
        .any(|v| !v.is_finite())
        || (snapshot.active && (snapshot.session == 0 || field.width <= 0.0 || field.height < 54.0))
    {
        return Err("Invalid canvas search state".into());
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_removes_private_query_and_field_but_preserves_warm_cards() {
        let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
        let results = state.canvas_search.result_windows.clone();
        assert_eq!(super::super::windows(&state).len(), results.len());
        let snapshot = Snapshot {
            version: 1,
            active: true,
            session: 9,
            query: "private document".into(),
            field: super::super::Bounds {
                width: 420.0,
                height: 54.0,
                ..Default::default()
            },
            ..Default::default()
        };
        apply(&mut state, Update::Snapshot(snapshot.clone()));
        assert_eq!(super::super::windows(&state).len(), results.len() + 1);
        let field = state.canvas_search.window_id;
        assert_eq!(state.canvas_search.result_windows, results);
        apply(&mut state, Update::Unavailable("disconnected".into()));
        assert_eq!(super::super::windows(&state).len(), results.len());
        assert!(state.canvas_search.query.is_empty());
        assert!(state.canvas_search.error.is_some());
        apply(&mut state, Update::Snapshot(snapshot));
        assert!(state.canvas_search.error.is_none());
        assert_ne!(state.canvas_search.window_id, field);
        assert_eq!(state.canvas_search.result_windows, results);
    }

    #[tokio::test]
    async fn registration_recovers_from_missing_socket_and_restarts_partial_batches() {
        use std::{
            io::{BufRead, BufReader, Write},
            os::unix::net::UnixListener,
        };
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("control.sock");
        assert!(!register(socket.clone()).await);
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            for (expected_offset, reply) in [
                (0, "ok\n"),
                (12, "err unavailable\n"),
                (0, "ok\n"),
                (12, "ok\n"),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let (name, payload) = line.split_once(' ').unwrap();
                assert_eq!(name, "canvas-search-register");
                let value: serde_json::Value = serde_json::from_str(payload).unwrap();
                assert_eq!(value["offset"], expected_offset);
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        assert!(!register(socket.clone()).await);
        assert!(register(socket).await);
        server.join().unwrap();
    }
}
