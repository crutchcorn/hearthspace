use std::{env, path::PathBuf};

pub(super) fn xdg_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(xdg_data_home) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        dirs.push(PathBuf::from(xdg_data_home));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share"));
    }

    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    dirs.extend(data_dirs);
    if env::var_os(hearthspace_common::DEV_ENV).is_some_and(|value| value == "1") {
        dirs.push(source_tree_data_dir());
    }
    dirs
}

pub(super) fn source_tree_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

pub(super) fn xdg_config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
        dirs.push(PathBuf::from(xdg_config_home));
    } else if let Some(home) = env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".config"));
    }

    let config_dirs = env::var_os("XDG_CONFIG_DIRS")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")]);
    dirs.extend(config_dirs);
    dirs
}

pub(super) fn executable_in_path(name: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|path| {
            let candidate = path.join(name);
            candidate.exists() && !candidate.is_dir()
        })
    })
}
