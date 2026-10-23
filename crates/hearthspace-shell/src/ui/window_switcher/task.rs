//! Coalesced updates and font shaping stay off the native UI event thread.
use super::{Prepared, ShellState, Snapshot, apply, text::Metrics};
use crate::ui::command_client;
use std::{path::Path, time::Duration};
use xilem::{
    core::{MessageProxy, NoElement, View},
    view::task_raw,
};

pub(in crate::ui) fn task() -> impl View<ShellState, (), xilem::ViewCtx, Element = NoElement> + use<>
{
    task_raw(
        |proxy: MessageProxy<Prepared>, state: &mut ShellState| {
            let socket = state.command_socket.clone();
            let refresh = state.window_switcher.refresh.clone();
            async move {
                let mut metrics = Metrics::default();
                let mut previous = None;
                let mut interval = tokio::time::interval(Duration::from_millis(500));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _ = refresh.notified() => {},
                        _ = interval.tick() => {},
                    }
                    let path = socket.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let result = prepare(&path, &mut metrics);
                        (metrics, result)
                    })
                    .await;
                    let Ok((returned_metrics, result)) = result else {
                        break;
                    };
                    metrics = returned_metrics;
                    // A disappeared socket/session must remove private title
                    // text immediately instead of retaining its last snapshot.
                    let prepared = result.unwrap_or_default();
                    if previous.as_ref() != Some(&prepared) {
                        previous = Some(prepared.clone());
                        if proxy.message(prepared).is_err() {
                            break;
                        }
                    }
                }
            }
        },
        apply,
    )
}

fn query(socket: &Path) -> Result<Snapshot, String> {
    let bytes =
        command_client::query_payload(socket, "window-switcher-status", Duration::from_millis(750))
            .map_err(|error| error.to_string())?;
    parse(&bytes)
}

fn prepare(socket: &Path, metrics: &mut Metrics) -> Result<Prepared, String> {
    // Selection may change while shaping or while acknowledging the measured
    // width. Only publish the title whose revision the compositor still shows.
    for _ in 0..3 {
        let snapshot = query(socket)?;
        if !snapshot.active {
            return Ok(Prepared {
                snapshot,
                ..Default::default()
            });
        }
        let (displayed, width) = metrics.prepare(&snapshot.title, snapshot.caption.max_width);
        if (width - snapshot.caption.width).abs() > 0.01 {
            let command = format!(
                "window-switcher-caption-size {}",
                serde_json::json!({
                    "session":snapshot.session,"revision":snapshot.revision,"width":width,
                })
            );
            if command_client::send_command(socket, &command).is_err() {
                continue;
            }
        }
        let current = query(socket)?;
        if !current.active {
            return Ok(Prepared {
                snapshot: current,
                ..Default::default()
            });
        }
        if current.session == snapshot.session && current.revision == snapshot.revision {
            return Ok(Prepared {
                snapshot: current,
                displayed,
            });
        }
    }
    Err("Window switcher changed while preparing its title".into())
}

fn parse(bytes: &[u8]) -> Result<Snapshot, String> {
    let mut snapshot: Snapshot =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let bounds = &snapshot.caption;
    if snapshot.version != 1
        || snapshot.title.len() > 16 * 1024
        || [
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            bounds.max_width,
        ]
        .iter()
        .any(|v| !v.is_finite())
        || (snapshot.active
            && (snapshot.session == 0
                || snapshot.index == 0
                || snapshot.index > snapshot.count
                || bounds.width <= 0.0
                || bounds.max_width <= 0.0
                || bounds.width > bounds.max_width
                || (bounds.height - 24.0).abs() > 0.01))
    {
        return Err("Invalid window switcher state".into());
    }
    if !snapshot.active {
        snapshot = Snapshot {
            version: snapshot.version,
            revision: snapshot.revision,
            ..Default::default()
        };
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
    };

    fn snapshot(session: u64, title: &str, width: f64) -> serde_json::Value {
        serde_json::json!({"version":1,"active":true,"session":session,"revision":4,
            "title":title,"index":1,"count":2,
            "caption":{"x":20,"y":30,"width":width,"height":24,"max_width":300}})
    }

    #[test]
    fn width_acknowledgement_cannot_publish_an_older_session_title() {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("control.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let mut measured_width = 0.0;
            // The first session disappears between its width acknowledgement
            // and the confirming query. Only the new session may reach the UI.
            for step in 0..6 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut command = String::new();
                BufReader::new(&stream).read_line(&mut command).unwrap();
                if step == 1 || step == 4 {
                    let data: serde_json::Value = serde_json::from_str(
                        command
                            .strip_prefix("window-switcher-caption-size ")
                            .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(data["session"], if step == 1 { 7 } else { 8 });
                    assert_eq!(data["revision"], 4);
                    measured_width = data["width"].as_f64().unwrap();
                    assert!((26.0..280.0).contains(&measured_width));
                    stream.write_all(b"ok\n").unwrap();
                } else {
                    assert_eq!(command, "window-switcher-status\n");
                    let value = if step == 0 {
                        snapshot(7, "Private previous session", 280.0)
                    } else {
                        snapshot(
                            8,
                            "New session",
                            if step == 5 { measured_width } else { 280.0 },
                        )
                    };
                    let bytes = serde_json::to_vec(&value).unwrap();
                    stream
                        .write_all(format!("ok {}\n", bytes.len()).as_bytes())
                        .unwrap();
                    stream.write_all(&bytes).unwrap();
                }
            }
        });
        let prepared = prepare(&socket, &mut Metrics::default()).unwrap();
        assert_eq!(prepared.snapshot.session, 8);
        assert_eq!(prepared.displayed, "New session");
        assert!(prepared.snapshot.caption.width < 280.0);
        server.join().unwrap();
    }

    #[test]
    fn inactive_and_invalid_geometry_never_create_a_caption_surface() {
        let inactive =
            parse(br#"{"version":1,"active":false,"title":"old title","caption":null}"#).unwrap();
        assert!(inactive.title.is_empty());
        assert_eq!(inactive.session, 0);
        let mut invalid = snapshot(7, "Notes", 0.0);
        assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
        invalid["caption"]["width"] = serde_json::json!(400);
        assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}
