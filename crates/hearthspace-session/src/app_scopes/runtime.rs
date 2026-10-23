//! Route Ptyxis's scope launcher through the existing session-owned guard.
use std::{env, ffi::OsString, fs, io, os::unix::fs::symlink, path::Path};

pub(super) fn prepare(runtime: &Path, guard: &Path) -> io::Result<()> {
    let guard = guard.canonicalize()?;
    let directory = runtime.join("app-bin");
    fs::create_dir(&directory)?;
    symlink(guard, directory.join("systemd-run"))
}

pub(crate) fn command_path(runtime: &Path, original: Option<OsString>) -> io::Result<String> {
    let original = original.unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
    let paths = std::iter::once(runtime.join("app-bin")).chain(env::split_paths(&original));
    env::join_paths(paths)
        .map_err(io::Error::other)?
        .into_string()
        .map_err(|_| io::Error::other("session PATH is not valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn private_path_resolves_the_adapter_and_preserves_other_commands() {
        let root = env::temp_dir().join(format!("hearthspace-scope-path-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let guard = root.join("guard");
        fs::write(&guard, "#!/bin/sh\nprintf '%s' \"$0\"\n").unwrap();
        fs::set_permissions(&guard, fs::Permissions::from_mode(0o700)).unwrap();
        prepare(&root, &guard).unwrap();
        let path = command_path(&root, Some("/usr/bin:/bin".into())).unwrap();
        let output = std::process::Command::new("systemd-run")
            .env("PATH", &path)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            root.join("app-bin/systemd-run").to_str().unwrap()
        );
        assert_eq!(
            which::which_in("true", Some(path), &root).unwrap(),
            Path::new("/usr/bin/true")
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
