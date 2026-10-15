use std::collections::HashMap;

use serde::Deserialize;

use super::AudioStatus;

const VOLUME_NORMAL: u64 = 65_536;
const VOLUME_MAX: u32 = u32::MAX / 2;
const INVALID: &str = "The audio service returned invalid output information.";
const NO_OUTPUT: &str = "No audio output is available.";

#[derive(Deserialize)]
struct ServerInfo {
    default_sink_name: Option<String>,
}

#[derive(Deserialize)]
struct Sink {
    name: String,
    description: Option<String>,
    mute: bool,
    volume: HashMap<String, Channel>,
}

#[derive(Deserialize)]
struct Channel {
    value: u32,
}

pub(super) struct Output {
    pub name: String,
    pub status: AudioStatus,
}

pub(super) fn parse_output(info: &[u8], sinks: &[u8]) -> Result<Output, String> {
    let info: ServerInfo = serde_json::from_slice(info).map_err(|_| INVALID)?;
    let name = info
        .default_sink_name
        .filter(|name| !name.is_empty())
        .ok_or(NO_OUTPUT)?;
    let sinks: Vec<serde_json::Value> = serde_json::from_slice(sinks).map_err(|_| INVALID)?;
    // Ignore unrelated outputs, including a malformed device that is not the
    // default. Missing or unplugged defaults must never appear as a fake 0%.
    let sink = sinks
        .into_iter()
        .find(|sink| sink.get("name").and_then(|value| value.as_str()) == Some(&name))
        .ok_or(NO_OUTPUT)?;
    let sink: Sink = serde_json::from_value(sink).map_err(|_| INVALID)?;
    let values: Vec<_> = sink.volume.values().map(|channel| channel.value).collect();
    if values.is_empty() || values.len() > 32 || values.iter().any(|&value| value > VOLUME_MAX) {
        return Err(INVALID.into());
    }
    // PulseAudio recommends the highest channel for a master slider. Do not
    // overwrite externally amplified volume simply because this UI stops at 100.
    let maximum = u64::from(*values.iter().max().expect("nonempty channels"));
    let volume = ((maximum * 100 + VOLUME_NORMAL / 2) / VOLUME_NORMAL).min(100) as u8;
    let output = sink
        .description
        .filter(|description| !description.trim().is_empty())
        .unwrap_or(sink.name);
    Ok(Output {
        name,
        status: AudioStatus {
            volume,
            muted: sink.mute,
            output,
        },
    })
}
