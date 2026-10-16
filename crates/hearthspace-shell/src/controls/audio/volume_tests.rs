use serde_json::json;

use super::test_support::Fixture;

#[tokio::test]
async fn master_volume_preserves_asymmetric_balance_in_device_channel_order() {
    let fixture = Fixture::new();
    let mut state = fixture.state();
    state["sinks"][0]["channel_map"] = json!("front-right,front-left");
    state["sinks"][0]["volume"]["front-left"]["value"] = json!(16_384);
    fixture.write(state);
    assert_eq!(fixture.backend.set_volume(100).await.unwrap().volume, 100);
    let state = fixture.state();
    assert_eq!(state["sinks"][0]["volume"]["front-left"]["value"], 32_768);
    assert_eq!(state["sinks"][0]["volume"]["front-right"]["value"], 65_536);
    assert!(
        fixture
            .commands()
            .iter()
            .any(|args| { args == &["--", "set-sink-volume", "speakers", "65536", "32768"] })
    );
}

#[tokio::test]
async fn a_deliberately_silent_channel_stays_silent_after_slider_and_key_changes() {
    let fixture = Fixture::new();
    let mut state = fixture.state();
    state["sinks"][0]["volume"]["front-left"]["value"] = json!(0);
    fixture.write(state);
    assert_eq!(
        fixture.backend.set_volume_unmuted(80).await.unwrap().volume,
        80
    );
    let actual = fixture
        .backend
        .step_volume_unmuted_guarded(-5, || async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(actual.volume, 75);
    assert!(!actual.muted);
    let state = fixture.state();
    assert_eq!(state["sinks"][0]["volume"]["front-left"]["value"], 0);
    assert_eq!(state["sinks"][0]["volume"]["front-right"]["value"], 49_152);
}

#[tokio::test]
async fn all_zero_channels_can_be_raised_and_relative_levels_stay_within_limits() {
    let fixture = Fixture::new();
    fixture.backend.set_volume(0).await.unwrap();
    let actual = fixture
        .backend
        .step_volume_unmuted_guarded(5, || async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(actual.volume, 5);
    assert!(!actual.muted);
    let state = fixture.state();
    assert_eq!(state["sinks"][0]["volume"]["front-left"]["value"], 3_277);
    assert_eq!(state["sinks"][0]["volume"]["front-right"]["value"], 3_277);
    assert_eq!(
        fixture
            .backend
            .step_volume_unmuted_guarded(i8::MAX, || async { Ok(()) })
            .await
            .unwrap()
            .volume,
        100
    );
    assert_eq!(
        fixture
            .backend
            .step_volume_unmuted_guarded(i8::MIN, || async { Ok(()) })
            .await
            .unwrap()
            .volume,
        0
    );
}

#[tokio::test]
async fn a_default_switch_cannot_apply_the_old_outputs_relative_level_to_its_replacement() {
    let fixture = Fixture::new();
    let mut first = true;
    let result = fixture
        .backend
        .step_volume_unmuted_guarded(5, || {
            if first {
                first = false;
                let mut state = fixture.state();
                state["default"] = json!("headphones");
                fixture.write(state);
            }
            async { Ok(()) }
        })
        .await;
    assert!(result.unwrap_err().contains("audio output changed"));
    let state = fixture.state();
    assert_eq!(state["sinks"][0]["volume"]["front-left"]["value"], 36_045);
    assert_eq!(state["sinks"][0]["mute"], false);
    assert_eq!(state["sinks"][1]["volume"]["front-left"]["value"], 16_384);
    assert_eq!(
        fixture
            .commands()
            .iter()
            .filter(|args| args == &&["--format=json", "info"])
            .count(),
        2,
        "one discovery and one readback; the step cannot rediscover a target before writing"
    );
    assert_eq!(
        fixture
            .backend
            .step_volume_unmuted_guarded(5, || async { Ok(()) })
            .await
            .unwrap()
            .volume,
        30
    );
}

#[tokio::test]
async fn malformed_channel_maps_cannot_reorder_or_fabricate_channels_for_a_write() {
    for map in [
        json!(""),
        json!("front-left"),
        json!("front-left,front-left"),
        json!("front-left,unknown"),
        json!(["front-left", "front-right"]),
    ] {
        let fixture = Fixture::new();
        let mut state = fixture.state();
        state["sinks"][0]["channel_map"] = map;
        fixture.write(state.clone());
        assert!(fixture.backend.set_volume(40).await.is_err());
        assert_eq!(fixture.state(), state);
        assert!(
            !fixture
                .commands()
                .iter()
                .any(|args| args.first().is_some_and(|arg| arg == "--"))
        );
    }
}
