//! Use the host user manager only to establish a confined app's tracking scope.
//! The command itself receives the private desktop runtime and D-Bus again.
use std::{env, fs, os::unix::fs::PermissionsExt, path::Path};

pub(super) fn wrap(instance: &str, command: Vec<String>) -> Result<Vec<String>, String> {
    let owner = env::var("HEARTHSPACE_APP_SCOPE_UNIT")
        .map_err(|_| "Snap launch requires a supervised native app scope owner".to_string())?;
    let runtime = env::var("XDG_RUNTIME_DIR").map_err(|error| error.to_string())?;
    let host =
        env::var(hearthspace_common::HOST_RUNTIME_DIR_ENV).map_err(|error| error.to_string())?;
    let bus = env::var("HEARTHSPACE_SESSION_BUS_ADDRESS").map_err(|error| error.to_string())?;
    let uuid =
        fs::read_to_string("/proc/sys/kernel/random/uuid").map_err(|error| error.to_string())?;
    let wrapped = wrap_with(
        instance,
        command,
        &owner,
        &runtime,
        &host,
        &bus,
        uuid.trim(),
    )?;
    // Register before any child can move out of the compositor's session cgroup.
    let records = Path::new(&runtime).join("app-scopes");
    fs::create_dir_all(&records).map_err(|error| error.to_string())?;
    if !fs::symlink_metadata(&records).is_ok_and(|meta| meta.file_type().is_dir()) {
        return Err("app scope records must be an owned directory".into());
    }
    fs::set_permissions(&records, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let unit = wrapped
        .iter()
        .find_map(|arg| arg.strip_prefix("--unit="))
        .ok_or("missing Snap tracking scope")?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(records.join(unit))
        .map_err(|error| error.to_string())?;
    Ok(wrapped)
}

fn wrap_with(
    instance: &str,
    command: Vec<String>,
    owner: &str,
    runtime: &str,
    host: &str,
    bus: &str,
    uuid: &str,
) -> Result<Vec<String>, String> {
    let tag = security_tag(instance, &command)?;
    if !owner.starts_with("hearthspace-apps-")
        || !owner.ends_with(".service")
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        || !Path::new(runtime).is_absolute()
        || !Path::new(host).is_absolute()
        || !bus.starts_with("unix:")
        || uuid.len() != 36
        || !uuid.bytes().enumerate().all(|(i, byte)| {
            if [8, 13, 18, 23].contains(&i) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
    {
        return Err("invalid owned Snap scope environment".into());
    }
    let mut wrapped = vec![
        "/usr/bin/env".into(),
        format!("XDG_RUNTIME_DIR={host}"),
        format!("DBUS_SESSION_BUS_ADDRESS=unix:path={host}/bus"),
        "/usr/bin/systemd-run".into(),
        "--user".into(),
        "--scope".into(),
        "--collect".into(),
        "--quiet".into(),
        "--no-ask-password".into(),
        "--expand-environment=no".into(),
        format!("--unit={tag}-{uuid}.scope"),
        format!("--property=BindsTo={owner}"),
        format!("--property=After={owner}"),
        "--property=TimeoutStopSec=1".into(),
        "/usr/bin/env".into(),
        format!("XDG_RUNTIME_DIR={runtime}"),
        format!("DBUS_SESSION_BUS_ADDRESS={bus}"),
    ];
    wrapped.extend(command);
    Ok(wrapped)
}

fn security_tag(instance: &str, command: &[String]) -> Result<String, String> {
    let valid = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && part.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte)
            })
    };
    if !valid(instance) {
        return Err("invalid Snap instance name".into());
    }
    // Exported desktop entries use /snap/bin/APP, sometimes behind env assignments.
    let mut args = command;
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "env" | "/usr/bin/env"))
    {
        args = &args[1..];
        while args
            .first()
            .is_some_and(|arg| arg.contains('=') && !arg.starts_with('-'))
        {
            args = &args[1..];
        }
    }
    let executable = args.first().ok_or("empty Snap launch command")?;
    let exported = if let Some(name) = executable.strip_prefix("/snap/bin/") {
        name
    } else if matches!(executable.as_str(), "snap" | "/usr/bin/snap")
        && args.get(1).map(String::as_str) == Some("run")
    {
        args.get(2).ok_or("missing snap run application")?
    } else {
        return Err("unsupported Snap launcher; expected /snap/bin/APP or snap run APP".into());
    };
    let (actual_instance, app) = exported.split_once('.').unwrap_or((exported, exported));
    // For a parallel instance, the default application retains the base snap name.
    let app = if !exported.contains('.') {
        app.split('_').next().unwrap()
    } else {
        app
    };
    if actual_instance != instance || !valid(app) || app.contains('_') {
        return Err("Snap desktop entry and executable identity differ".into());
    }
    Ok(format!("snap.{instance}.{app}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_exact_snap_security_tags_and_rejects_mismatches() {
        let tag = |instance, argv: &[&str]| {
            security_tag(
                instance,
                &argv.iter().map(|arg| (*arg).into()).collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            tag("firefox", &["/snap/bin/firefox"]).unwrap(),
            "snap.firefox.firefox"
        );
        assert_eq!(
            tag(
                "foo_test",
                &[
                    "env",
                    "BAMF_DESKTOP_FILE_HINT=/x",
                    "/snap/bin/foo_test.editor"
                ]
            )
            .unwrap(),
            "snap.foo_test.editor"
        );
        assert_eq!(
            tag("foo_test", &["/snap/bin/foo_test"]).unwrap(),
            "snap.foo_test.foo"
        );
        assert_eq!(
            tag("foo", &["snap", "run", "foo.editor"]).unwrap(),
            "snap.foo.editor"
        );
        assert!(tag("firefox", &["/snap/bin/other"]).is_err());
        assert!(tag("firefox", &["/bin/sh", "-c", "/snap/bin/firefox"]).is_err());
    }

    #[test]
    fn manager_environment_is_restored_before_the_unchanged_app_argv() {
        let original = vec!["/snap/bin/firefox".into(), "file:///tmp/a $b.txt".into()];
        let wrapped = wrap_with(
            "firefox",
            original.clone(),
            "hearthspace-apps-test.service",
            "/private",
            "/run/user/1000",
            "unix:path=/tmp/private-bus",
            "01234567-89ab-cdef-0123-456789abcdef",
        )
        .unwrap();
        assert_eq!(&wrapped[wrapped.len() - original.len()..], &original);
        assert_eq!(wrapped[1], "XDG_RUNTIME_DIR=/run/user/1000");
        assert!(wrapped.contains(&"XDG_RUNTIME_DIR=/private".into()));
        assert!(wrapped.contains(&"DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/private-bus".into()));
        assert!(wrapped.contains(&"--property=BindsTo=hearthspace-apps-test.service".into()));
        assert!(wrapped.contains(&"--expand-environment=no".into()));
    }
}
