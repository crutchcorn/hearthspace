//! A single worker serializes layout commands and coalesces semantic updates.
use std::time::Duration;
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

use super::{ShellState, Snapshot, apply};

#[derive(Debug)]
enum Update {
    Snapshot(Box<Snapshot>),
    Error(String),
    Open(String, super::popups::Popup),
}

#[derive(Debug)]
pub(super) struct Request {
    pub command: String,
    pub open: Option<(String, super::popups::Popup)>,
}

pub(in crate::ui) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<Update>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            let refresh = state.frame_layout.refresh.clone();
            let (sender, mut requests) = tokio::sync::mpsc::channel::<Request>(32);
            state.frame_layout.requests = Some(sender);
            let (availability_sender, mut availability) =
                tokio::sync::watch::channel(Vec::<(String, String, bool)>::new());
            state.frame_layout.availability = Some(availability_sender);
            async move {
                let mut interval = tokio::time::interval(Duration::from_millis(500));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut previous = None;
                let mut published_availability = std::collections::BTreeMap::new();
                loop {
                    let request = tokio::select! {
                        _ = interval.tick() => None,
                        _ = refresh.notified() => None,
                        _ = availability.changed() => None,
                        value = requests.recv() => { let Some(value) = value else { break }; Some(value) },
                    };
                    let open = request.as_ref().and_then(|request| request.open.clone());
                    let available = availability.borrow_and_update().clone();
                    let path = socket.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        for (output,id,available) in available {
                            let key=(output.clone(),id.clone());
                            if published_availability.get(&key)!=Some(&available) {
                                let command=format!("frame-widget-available {}",serde_json::json!({"output":output,"id":id,"available":available}));
                                if crate::ui::command_client::send_command(&path,&command).is_ok(){published_availability.insert(key,available);}
                            }
                        }
                        let result=(|| {
                        if let Some(request) = request {
                            crate::ui::command_client::send_command(&path, &request.command)?;
                        }
                        let bytes = crate::ui::command_client::query_frame_layout(&path)
                            .map_err(|error| error.to_string())?;
                        parse(&bytes)
                        })();
                        (published_availability,result)
                    }).await;
                    let Ok((published, result)) = result else {
                        break;
                    };
                    published_availability = published;
                    match result {
                        Ok(snapshot) => {
                            let may_open = !snapshot.editing
                                && snapshot.confirmation.is_none()
                                && !snapshot.outputs.is_empty();
                            if previous.as_ref() != Some(&snapshot) {
                                previous = Some(snapshot.clone());
                                if proxy.message(Update::Snapshot(Box::new(snapshot))).is_err() {
                                    break;
                                }
                            }
                            if may_open
                                && let Some((key, popup)) = open
                                && proxy.message(Update::Open(key, popup)).is_err()
                            {
                                break;
                            }
                        }
                        Err(error) => {
                            // Retain ordinary frame parts, but never keep a stale editor modal.
                            if previous
                                .as_ref()
                                .is_some_and(|old| old.editing || old.confirmation.is_some())
                            {
                                let mut snapshot = previous.clone().unwrap_or_default();
                                snapshot.editing = false;
                                snapshot.session = 0;
                                snapshot.selected = None;
                                snapshot.confirmation = None;
                                snapshot.focus_request = None;
                                previous = Some(snapshot.clone());
                                if proxy.message(Update::Snapshot(Box::new(snapshot))).is_err() {
                                    break;
                                }
                            }
                            if proxy.message(Update::Error(error)).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        },
        |state, update| match update {
            Update::Snapshot(snapshot) => {
                state.frame_layout.error = None;
                apply(state, *snapshot);
            }
            Update::Error(error) => state.frame_layout.error = Some(error),
            Update::Open(key, popup) => super::popups::open(state, &key, popup),
        },
    )
}

fn parse(bytes: &[u8]) -> Result<Snapshot, String> {
    let snapshot: Snapshot = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if snapshot.version != 1
        || snapshot.outputs.len() > 64
        || (snapshot.editing && snapshot.session == 0)
    {
        return Err("Invalid frame layout state".into());
    }
    for output in &snapshot.outputs {
        output.layout.validate().map_err(str::to_string)?;
        if output.widgets.len() > hearthspace_common::frame_layout::WIDGETS.len()
            || output.groups.len() > 1024
        {
            return Err("Invalid frame layout parts".into());
        }
        for rect in std::iter::once(output.rect)
            .chain(output.widgets.iter().map(|part| part.rect))
            .chain(
                output
                    .groups
                    .iter()
                    .flat_map(|group| [group.rect, group.grip]),
            )
        {
            if ![rect.x, rect.y, rect.width, rect.height]
                .into_iter()
                .all(f64::is_finite)
                || rect.width < 0.0
                || rect.height < 0.0
            {
                return Err("Invalid frame layout geometry".into());
            }
        }
    }
    Ok(snapshot)
}
