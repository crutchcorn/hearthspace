//! Keep user audio reachable while Wayland and D-Bus use a private runtime.
use std::env;

pub(crate) fn audio_environment(host_runtime_dir: &str) -> Vec<(String, String)> {
    defaults(host_runtime_dir, |key| env::var_os(key).is_some())
}

fn defaults(host_runtime_dir: &str, is_set: impl Fn(&str) -> bool) -> Vec<(String, String)> {
    let host = host_runtime_dir.trim_end_matches('/');
    let mut vars = Vec::new();
    // An explicit server or alternate runtime is already the user's routing
    // choice. Do not replace either, including deliberately empty values.
    if !is_set("PULSE_SERVER") && !is_set("PULSE_RUNTIME_PATH") {
        vars.push((
            "PULSE_SERVER".to_string(),
            format!("unix:{host}/pulse/native"),
        ));
    }
    if !is_set("PIPEWIRE_RUNTIME_DIR") {
        // Relative remote names still resolve against the original runtime.
        vars.push((
            "PIPEWIRE_RUNTIME_DIR".to_string(),
            host_runtime_dir.to_string(),
        ));
        if !is_set("PIPEWIRE_REMOTE") {
            vars.push(("PIPEWIRE_REMOTE".to_string(), format!("{host}/pipewire-0")));
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_sessions_use_absolute_host_audio_sockets() {
        assert_eq!(
            defaults("/run/user/1000", |_| false),
            [
                (
                    "PULSE_SERVER".into(),
                    "unix:/run/user/1000/pulse/native".into()
                ),
                ("PIPEWIRE_RUNTIME_DIR".into(), "/run/user/1000".into()),
                ("PIPEWIRE_REMOTE".into(), "/run/user/1000/pipewire-0".into()),
            ]
        );
    }

    #[test]
    fn explicit_servers_are_preserved_and_relative_pipewire_names_keep_their_runtime() {
        assert_eq!(
            defaults("/run/user/1000", |key| matches!(
                key,
                "PULSE_SERVER" | "PIPEWIRE_REMOTE"
            )),
            [("PIPEWIRE_RUNTIME_DIR".into(), "/run/user/1000".into())]
        );
    }

    #[test]
    fn custom_audio_runtimes_are_not_overridden() {
        assert!(
            defaults("/run/user/1000", |key| matches!(
                key,
                "PULSE_RUNTIME_PATH" | "PIPEWIRE_RUNTIME_DIR"
            ))
            .is_empty()
        );
    }

    #[test]
    fn trailing_runtime_slash_does_not_duplicate_socket_separators() {
        let vars = defaults("/run/user/1000/", |_| false);
        assert_eq!(vars[0].1, "unix:/run/user/1000/pulse/native");
        assert_eq!(vars[2].1, "/run/user/1000/pipewire-0");
    }
}
