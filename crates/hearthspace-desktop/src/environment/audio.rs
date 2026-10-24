//! Complete audio routing for direct children, activation, and broker retirement.
pub fn resolved(host: &str, get: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let host = host.trim_end_matches('/');
    let pulse_runtime = get("PULSE_RUNTIME_PATH").unwrap_or_else(|| format!("{host}/pulse"));
    let pulse_server = get("PULSE_SERVER").unwrap_or_else(|| {
        // An explicitly empty runtime is invalid; don't turn it into /native.
        if pulse_runtime.is_empty() {
            String::new()
        } else {
            format!("unix:{}/native", pulse_runtime.trim_end_matches('/'))
        }
    });
    let pipewire_runtime = get("PIPEWIRE_RUNTIME_DIR").unwrap_or_else(|| host.into());
    let pipewire_remote = get("PIPEWIRE_REMOTE").unwrap_or_else(|| "pipewire-0".into());
    let pipewire_core = get("PIPEWIRE_CORE").unwrap_or_else(|| "pipewire-0".into());
    // Empty is not unset for these libraries. Preserve explicit values and fill
    // absent complements with concrete defaults, never generic empty strings.
    vec![
        ("PULSE_SERVER".into(), pulse_server),
        ("PULSE_RUNTIME_PATH".into(), pulse_runtime),
        ("PIPEWIRE_RUNTIME_DIR".into(), pipewire_runtime),
        ("PIPEWIRE_REMOTE".into(), pipewire_remote),
        ("PIPEWIRE_CORE".into(), pipewire_core),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn private_sessions_keep_host_audio_and_resolve_relative_remote_against_it() {
        let vars: BTreeMap<_, _> = resolved("/run/user/1000/", |_| None).into_iter().collect();
        assert_eq!(vars["PULSE_SERVER"], "unix:/run/user/1000/pulse/native");
        assert_eq!(vars["PULSE_RUNTIME_PATH"], "/run/user/1000/pulse");
        assert_eq!(vars["PIPEWIRE_RUNTIME_DIR"], "/run/user/1000");
        assert_eq!(vars["PIPEWIRE_REMOTE"], "pipewire-0");
        assert_eq!(vars["PIPEWIRE_CORE"], "pipewire-0");
    }

    #[test]
    fn explicit_empty_values_stay_empty_including_invalid_runtime_overrides() {
        let vars: BTreeMap<_, _> = resolved("/host", |_| Some(String::new()))
            .into_iter()
            .collect();
        assert!(vars.values().all(String::is_empty));
        let vars: BTreeMap<_, _> = resolved("/host", |key| {
            (key == "PULSE_RUNTIME_PATH").then(String::new)
        })
        .into_iter()
        .collect();
        assert_eq!(vars["PULSE_SERVER"], "");
        assert_eq!(vars["PULSE_RUNTIME_PATH"], "");
    }
}
