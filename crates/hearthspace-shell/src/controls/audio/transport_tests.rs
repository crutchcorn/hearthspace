use std::{fs, time::Duration};

use serde_json::{Value, json};

use super::{AudioBackend, test_support::Fixture};

#[tokio::test]
async fn volume_writes_are_bounded_and_mute_is_independent() {
    let fixture = Fixture::new();
    let status = fixture.backend.set_volume(150).await.unwrap();
    assert_eq!(status.volume, 100);
    assert!(status.muted);
    let commands = fixture.commands();
    assert!(
        commands
            .iter()
            .any(|args| args == &["--", "set-sink-volume", "speakers", "100%"])
    );
    assert!(
        !commands
            .iter()
            .any(|args| args.iter().any(|arg| arg == "set-sink-mute"))
    );
}

#[tokio::test]
async fn positive_slider_adjustment_unmutes_the_same_output() {
    let fixture = Fixture::new();
    let status = fixture.backend.set_volume_unmuted(65).await.unwrap();
    assert_eq!(status.volume, 65);
    assert!(!status.muted);
    let state = fixture.state();
    assert_eq!(state["sinks"][1]["volume"]["front-left"]["value"], 16_384);
    assert_eq!(state["sinks"][1]["mute"], false);
    let commands: Vec<_> = fixture
        .commands()
        .into_iter()
        .filter(|args| args.first().is_some_and(|arg| arg == "--"))
        .collect();
    assert_eq!(
        commands,
        [
            ["--", "set-sink-volume", "speakers", "65%"],
            ["--", "set-sink-mute", "speakers", "0"],
        ]
    );
}

#[tokio::test]
async fn zero_does_not_change_mute_and_explicit_mute_controls_work() {
    let fixture = Fixture::new();
    assert!(fixture.backend.set_volume_unmuted(0).await.unwrap().muted);
    assert!(!fixture.backend.toggle_mute().await.unwrap().muted);
    assert!(fixture.backend.set_muted(true).await.unwrap().muted);
    assert!(!fixture.backend.set_muted(false).await.unwrap().muted);
}

#[tokio::test]
async fn reconnect_and_default_changes_do_not_require_recreating_backend() {
    let fixture = Fixture::new();
    assert_eq!(fixture.backend.status().await.unwrap().volume, 50);
    let mut state = fixture.state();
    state["mode"] = json!("server-down");
    fixture.write(state.clone());
    assert_eq!(
        fixture.backend.status().await.unwrap_err(),
        "The audio service is unavailable."
    );
    state["mode"] = Value::Null;
    state["default"] = json!("headphones");
    fixture.write(state);
    assert_eq!(fixture.backend.status().await.unwrap().volume, 25);
    assert_eq!(fixture.backend.set_volume(40).await.unwrap().volume, 40);
    assert_eq!(
        fixture.state()["sinks"][0]["volume"]["front-left"]["value"],
        32_768
    );
}

#[tokio::test]
async fn change_does_not_redirect_to_replacement_default_mid_operation() {
    let fixture = Fixture::new();
    let mut state = fixture.state();
    state["switch_on_change"] = json!("headphones");
    fixture.write(state);
    assert!(
        fixture
            .backend
            .set_volume_unmuted(60)
            .await
            .unwrap_err()
            .contains("audio output changed")
    );
    let state = fixture.state();
    assert_eq!(state["sinks"][0]["mute"], false);
    assert_eq!(state["sinks"][1]["volume"]["front-left"]["value"], 16_384);
    assert_eq!(fixture.backend.status().await.unwrap().volume, 25);
}

#[tokio::test]
async fn disappearing_output_is_a_clean_failure_without_fallback_mutations() {
    let fixture = Fixture::new();
    let mut state = fixture.state();
    state["remove_on_change"] = json!(true);
    fixture.write(state.clone());
    assert_eq!(
        fixture.backend.set_volume_unmuted(60).await.unwrap_err(),
        "No audio output is available."
    );
    assert_eq!(fixture.state(), state);
}

#[tokio::test]
async fn permission_revoked_during_discovery_prevents_volume_and_mute_writes() {
    let fixture = Fixture::new();
    let original = fixture.state();
    let mut guarded = false;
    let result = fixture
        .backend
        .set_volume_unmuted_guarded(70, || {
            guarded = true;
            assert!(
                fixture
                    .commands()
                    .iter()
                    .any(|args| args == &["--format=json", "info"])
            );
            async { Err("The desktop became inactive.".into()) }
        })
        .await;
    assert_eq!(result.unwrap_err(), "The desktop became inactive.");
    assert!(guarded);
    assert_eq!(fixture.state(), original);
    assert!(
        !fixture
            .commands()
            .iter()
            .any(|args| args.first().is_some_and(|arg| arg == "--"))
    );

    assert!(
        fixture
            .backend
            .toggle_mute_guarded(|| async { Err("The desktop is locked.".into()) })
            .await
            .is_err()
    );
    assert_eq!(fixture.state(), original);
}

#[tokio::test]
async fn permission_is_checked_again_before_unmuting_after_volume_write() {
    let fixture = Fixture::new();
    let mut calls = 0;
    let result = fixture
        .backend
        .set_volume_unmuted_guarded(70, || {
            calls += 1;
            let allowed = calls == 1;
            async move {
                if allowed {
                    Ok(())
                } else {
                    Err("The desktop became inactive.".into())
                }
            }
        })
        .await;
    assert_eq!(result.unwrap_err(), "The desktop became inactive.");
    assert_eq!(calls, 2);
    let reading = fixture.backend.status().await.unwrap();
    assert_eq!(reading.volume, 70);
    assert!(
        reading.muted,
        "a stale operation must not unmute the output"
    );
    assert!(
        !fixture
            .commands()
            .iter()
            .any(|args| args.iter().any(|arg| arg == "set-sink-mute"))
    );
}

#[tokio::test]
async fn external_audio_routing_is_preserved_and_device_names_are_plain_arguments() {
    let fixture = Fixture::new();
    let mut state = fixture.state();
    let name = "--server=wrong; $(touch should-not-exist)";
    state["default"] = json!(name);
    state["sinks"][0]["name"] = json!(name);
    fixture.write(state);
    fixture.backend.set_volume(35).await.unwrap();
    let routing: Value =
        serde_json::from_slice(&fs::read(fixture.file("routing.json")).unwrap()).unwrap();
    for key in ["PULSE_SERVER", "PULSE_RUNTIME_PATH", "PULSE_COOKIE"] {
        assert_eq!(routing[key], json!(std::env::var(key).ok()));
    }
    assert!(
        fixture
            .commands()
            .iter()
            .any(|args| args == &["--", "set-sink-volume", name, "35%"])
    );
}

#[tokio::test]
async fn missing_client_is_reported_without_panicking() {
    let directory = tempfile::tempdir().unwrap();
    let backend = AudioBackend {
        program: directory.path().join("missing-pactl"),
        ..Default::default()
    };
    assert_eq!(
        backend.status().await.unwrap_err(),
        "Volume controls require pactl (pulseaudio-utils)."
    );
}

#[tokio::test]
async fn stalled_client_is_killed_reaped_and_recoverable() {
    let mut fixture = Fixture::new();
    fixture.backend.timeout = Duration::from_millis(500);
    let mut state = fixture.state();
    state["mode"] = json!("stall");
    fixture.write(state.clone());
    let start = tokio::time::Instant::now();
    assert_eq!(
        fixture.backend.run(&["info"]).await.unwrap_err(),
        "The audio service is not responding."
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_children_exited(&fixture).await;
    state["mode"] = Value::Null;
    fixture.write(state);
    assert_eq!(fixture.backend.status().await.unwrap().volume, 50);
}

#[tokio::test]
async fn excessive_output_is_bounded_and_client_is_reaped() {
    let fixture = Fixture::new();
    let mut state = fixture.state();
    state["mode"] = json!("oversized");
    fixture.write(state);
    assert_eq!(
        fixture.backend.run(&["info"]).await.unwrap_err(),
        "The audio service response was too large."
    );
    assert_children_exited(&fixture).await;
}

#[tokio::test]
async fn cancelling_a_request_does_not_leave_the_client_running() {
    let fixture = Fixture::new();
    let mut state = fixture.state();
    state["mode"] = json!("stall");
    fixture.write(state);
    let backend = fixture.backend.clone();
    let task = tokio::spawn(async move { backend.run(&["info"]).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while !fixture.file("pids").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_children_exited(&fixture).await;
}

async fn assert_children_exited(fixture: &Fixture) {
    let pids: Vec<u32> = fs::read_to_string(fixture.file("pids"))
        .unwrap()
        .lines()
        .map(|pid| pid.parse().unwrap())
        .collect();
    assert!(!pids.is_empty());
    tokio::time::timeout(Duration::from_secs(2), async {
        while pids
            .iter()
            .any(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists())
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("audio helper process should be killed and reaped");
}
