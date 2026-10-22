//! Recognize exported Snap commands even when a shortcut lacks Snap metadata.
use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use smithay::reexports::rustix::fs::{Access, AtFlags, CWD, accessat};

pub(in crate::compositor::shell_integration::launch) fn resolve(
    instance: Option<&str>,
    command: &mut [String],
    working_directory: Option<&Path>,
) -> Result<Option<String>, String> {
    let current = env::current_dir().map_err(|error| error.to_string())?;
    let cwd = working_directory.map_or(current.clone(), |directory| current.join(directory));
    let path = env::var_os("PATH").unwrap_or_else(|| "/bin:/usr/bin".into());
    resolve_with(instance, command, &cwd, &path, Path::new("/snap/bin"))
}

fn resolve_with(
    instance: Option<&str>,
    command: &mut [String],
    cwd: &Path,
    path: &OsStr,
    snap_bin: &Path,
) -> Result<Option<String>, String> {
    let unchanged = || Ok(instance.map(str::to_owned));
    let Some(program) = command.first() else {
        return unchanged();
    };
    let Some(mut resolved) = executable(program, cwd, path) else {
        return unchanged();
    };
    let mut index = 0;
    // Only the system env utility has these semantics. An earlier PATH entry
    // named env may be an unrelated program and must retain its own arguments.
    let system_env = fs::canonicalize("/usr/bin/env").ok();
    if system_env.is_some() && fs::canonicalize(&resolved).ok() == system_env {
        index += 1;
        if command.get(index).is_some_and(|argument| argument == "--") {
            index += 1;
        }
        let mut effective_path = path.to_owned();
        while let Some(argument) = command.get(index) {
            if argument.starts_with('-') {
                // Options such as -C or -S change lookup/argument semantics.
                return unchanged();
            }
            let Some((key, value)) = argument.split_once('=') else {
                break;
            };
            if key == "PATH" {
                effective_path = value.into();
            }
            index += 1;
        }
        let Some(program) = command.get(index) else {
            return unchanged();
        };
        let Some(found) = executable(program, cwd, &effective_path) else {
            return unchanged();
        };
        resolved = found;
    }
    let Some(export_directory) = fs::canonicalize(snap_bin).ok() else {
        return unchanged();
    };
    if resolved
        .parent()
        .and_then(|parent| fs::canonicalize(parent).ok())
        .as_ref()
        != Some(&export_directory)
    {
        return unchanged();
    }
    let exported = resolved
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or("Snap exported executable must have a UTF-8 name")?;
    let inferred = exported.split('.').next().unwrap();
    // Keep the exported name: resolving its final symlink would turn steam into
    // the generic snap binary and lose the security tag and launch semantics.
    let normalized = format!("/snap/bin/{exported}");
    super::security_tag(
        instance.unwrap_or(inferred),
        std::slice::from_ref(&normalized),
    )?;
    command[index] = normalized;
    if index > 0 {
        // The wrapper was verified as the system env executable. Keep the
        // scope parser's supported spelling, including for /bin/env aliases.
        command[0] = "/usr/bin/env".into();
    }
    Ok(Some(inferred.to_owned()))
}

fn executable(program: &str, cwd: &Path, path: &OsStr) -> Option<PathBuf> {
    let usable = |candidate: &Path| {
        candidate.is_file() && accessat(CWD, candidate, Access::EXEC_OK, AtFlags::EACCESS).is_ok()
    };
    if program.contains('/') {
        let candidate = cwd.join(program);
        return usable(&candidate).then_some(candidate);
    }
    env::split_paths(path)
        .map(|directory| cwd.join(directory).join(program))
        .find(|candidate| usable(candidate))
}

#[cfg(test)]
mod tests;
