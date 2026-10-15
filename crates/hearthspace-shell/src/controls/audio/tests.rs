use serde_json::{Value, json};

use super::{AudioStatus, model::parse_output};

pub(super) fn sink(name: &str, volume: u32, muted: bool) -> Value {
    json!({
        "name": name,
        "description": "Speakers",
        "mute": muted,
        "volume": {
            "front-left": { "value": volume, "value_percent": "localized text" },
            "front-right": { "value": volume },
        },
    })
}

fn parse(default: Value, sinks: Value) -> Result<AudioStatus, String> {
    parse_output(
        &serde_json::to_vec(&json!({ "default_sink_name": default })).unwrap(),
        &serde_json::to_vec(&sinks).unwrap(),
    )
    .map(|output| output.status)
}

#[test]
fn default_sink_and_highest_channel_drive_the_control() {
    let mut selected = sink("speakers", 16_384, true);
    selected["volume"]["front-right"]["value"] = json!(32_768);
    let status = parse(
        json!("speakers"),
        json!([sink("headphones", 0, false), selected]),
    )
    .unwrap();
    assert_eq!(
        status,
        AudioStatus {
            volume: 50,
            muted: true,
            output: "Speakers".into(),
        }
    );
}

#[test]
fn normal_silent_and_externally_amplified_volumes_are_displayed_safely() {
    for (raw, expected) in [(0, 0), (24_903, 38), (65_536, 100), (98_304, 100)] {
        assert_eq!(
            parse(json!("default"), json!([sink("default", raw, false)]))
                .unwrap()
                .volume,
            expected
        );
    }
}

#[test]
fn missing_empty_and_disappeared_defaults_are_unavailable() {
    for default in [Value::Null, json!(""), json!("unplugged")] {
        assert_eq!(
            parse(default, json!([sink("speakers", 65_536, false)])).unwrap_err(),
            "No audio output is available."
        );
    }
    assert!(parse(json!("speakers"), json!([])).is_err());
}

#[test]
fn malformed_default_volume_or_mute_is_not_reported_as_zero() {
    let original = sink("default", 32_768, false);
    for invalid in [
        Value::Null,
        json!({}),
        json!({"front-left": { "value": -1 }}),
        json!({"front-left": { "value": "50%" }}),
        json!({"front-left": { "value": u32::MAX }}),
    ] {
        let mut output = original.clone();
        output["volume"] = invalid;
        assert!(parse(json!("default"), json!([output])).is_err());
    }
    let mut output = original;
    output.as_object_mut().unwrap().remove("mute");
    assert!(parse(json!("default"), json!([output])).is_err());
}

#[test]
fn descriptions_are_optional_and_unrelated_malformed_outputs_are_ignored() {
    let mut output = sink("my-device", 65_536, false);
    output["description"] = json!("   ");
    assert_eq!(
        parse(json!("my-device"), json!([{"name":"broken"}, output]))
            .unwrap()
            .output,
        "my-device"
    );
}

#[test]
fn invalid_json_and_unexpected_shapes_are_rejected() {
    for (info, sinks) in [
        (b"{bad".as_slice(), b"[]".as_slice()),
        (b"null", b"[]"),
        (b"{\"default_sink_name\":\"test\"}", b"{}"),
        (b"{\"default_sink_name\":42}", b"[]"),
    ] {
        assert!(parse_output(info, sinks).is_err());
    }
}
