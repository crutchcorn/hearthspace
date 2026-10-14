//! Harness-only defaults for apps activated without the compositor's launcher.
use std::{env, fs, io, path::Path};

use crate::session_env::SessionEnv;

pub(crate) fn for_session(session: &SessionEnv) -> io::Result<Vec<(String, String)>> {
    let isolated =
        env::var_os("HEARTHSPACE_ISOLATE_APP_PROFILES").is_some_and(|value| value == "1");
    if !isolated {
        return Ok(Vec::new());
    }
    let runtime = session
        .vars
        .iter()
        .find(|(key, _)| key == "XDG_RUNTIME_DIR")
        .map(|(_, value)| value)
        .ok_or_else(|| io::Error::other("private runtime is missing"))?;
    environment(Path::new(runtime), isolated)
}

fn environment(runtime: &Path, isolated: bool) -> io::Result<Vec<(String, String)>> {
    if !isolated {
        return Ok(Vec::new());
    }
    let profile = runtime.join("activated-app-profile");
    let mut envs = Vec::new();
    for (variable, leaf) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = profile.join(leaf);
        fs::create_dir_all(&path)?;
        envs.push((variable.into(), path.to_string_lossy().into_owned()));
    }
    // dconf is a separately activated process and otherwise uses the ordinary
    // session configuration even for clients with a private XDG_CONFIG_HOME.
    envs.push(("GSETTINGS_BACKEND".into(), "keyfile".into()));
    Ok(envs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_sessions_keep_their_real_profile_defaults() {
        assert!(
            environment(Path::new("/this-path-must-not-be-created"), false)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn isolated_activation_profiles_stay_inside_the_owned_runtime() {
        let runtime = env::temp_dir().join(format!(
            "hearthspace-activated-profile-{}",
            std::process::id()
        ));
        let envs = environment(&runtime, true).unwrap();
        for (_, path) in &envs[..4] {
            assert!(Path::new(path).starts_with(&runtime));
            assert!(Path::new(path).is_dir());
        }
        assert_eq!(envs[4], ("GSETTINGS_BACKEND".into(), "keyfile".into()));
        fs::remove_dir_all(runtime).unwrap();
    }
}
