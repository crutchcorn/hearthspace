use super::*;
use std::{
    os::unix::{ffi::OsStringExt, fs::symlink},
    sync::atomic::{AtomicU64, Ordering},
};

const UNIT: &str = "ptyxis-spawn-84a59847-c85f-47b0-bd77-3f5af034184c.scope";
const UNIT_ARG: &str = "--unit=ptyxis-spawn-84a59847-c85f-47b0-bd77-3f5af034184c.scope";

struct Fixture {
    root: PathBuf,
    runtime: PathBuf,
    host: PathBuf,
    bus: OsString,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = env::temp_dir().join(format!(
            "hearthspace-ptyxis-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Self {
            runtime: root.join("private"),
            host: root.join("host= space%"),
            root,
            bus: "unix:path=/private%20runtime/bus,guid=0123456789abcdef".into(),
        };
        for directory in [&fixture.root, &fixture.runtime, &fixture.host] {
            fs::DirBuilder::new().mode(0o700).create(directory).unwrap();
        }
        fs::write(
            fixture.runtime.join("app-scope-owner"),
            identity::owner_unit(&fixture.runtime).unwrap(),
        )
        .unwrap();
        fs::write(fixture.runtime.join(identity::READY_FILE), b"ready").unwrap();
        fixture
    }

    fn session(&self) -> io::Result<Session> {
        Session::load(self.runtime.clone(), self.host.clone(), self.bus.clone())
    }

    fn record(&self) -> PathBuf {
        self.runtime.join("app-scopes").join(UNIT)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn invocation(command: &[&str]) -> Vec<OsString> {
    PREFIX
        .iter()
        .copied()
        .chain([UNIT_ARG])
        .chain(command.iter().copied())
        .map(OsString::from)
        .collect()
}

#[test]
fn only_the_known_ptyxis_scope_prefix_is_intercepted() {
    let args = invocation(&["/bin/bash", "--login"]);
    let scope = scope_invocation(&args).unwrap();
    assert_eq!(scope.unit, UNIT);
    assert_eq!(&args[scope.command_start..], ["/bin/bash", "--login"]);

    let unrelated: &[(&str, &[&str])] = &[
        ("version probe", &["--version"]),
        ("no command", &[]),
        (
            "extra manager option",
            &["--property=Description=custom", "/bin/sh"],
        ),
        ("missing command after separator", &["--"]),
        ("empty command", &[""]),
    ];
    for (case, command) in unrelated {
        let args = if *case == "version probe" {
            command.iter().map(OsString::from).collect()
        } else {
            invocation(command)
        };
        assert!(scope_invocation(&args).is_none(), "{case}");
    }

    let mut reordered = invocation(&["/bin/sh"]);
    reordered.swap(0, 1);
    assert!(scope_invocation(&reordered).is_none());
}

#[test]
fn invalid_or_unrelated_unit_names_pass_through() {
    for unit in [
        "snap.terminal.scope",
        "ptyxis-spawn-arbitrary.scope",
        "ptyxis-spawn-84a59847-c85f-47b0-bd77-3f5af034184z.scope",
        "ptyxis-spawn-84a59847_c85f-47b0-bd77-3f5af034184c.scope",
        "ptyxis-spawn-../../outside.scope",
    ] {
        let mut args = invocation(&["/bin/sh"]);
        args[PREFIX.len()] = format!("--unit={unit}").into();
        assert!(scope_invocation(&args).is_none(), "{unit}");
    }
}

#[test]
fn host_scope_keeps_the_private_shell_environment_and_literal_arguments() {
    let fixture = Fixture::new();
    let session = fixture.session().unwrap();
    let mut args = invocation(&[
        "--",
        "/bin/sh",
        "-c",
        "printf '%s\\0' \"$XDG_RUNTIME_DIR\" \"$DBUS_SESSION_BUS_ADDRESS\" \"$@\"",
        "shell",
        "$XDG_RUNTIME_DIR; $(exit 23)",
        "--property=Description=literal-shell-argument",
    ]);
    args.push(OsString::from_vec(b"non-unicode-\xff".to_vec()));
    let scope = scope_invocation(&args).unwrap();
    let command = session.prepare(&args, &scope).unwrap();
    assert!(fixture.record().is_file());
    assert_eq!(command.get_program(), SYSTEMD_RUN);
    let manager_args = command.get_args().collect::<Vec<_>>();
    assert_eq!(
        &manager_args[..4],
        [
            "--property=BindsTo=hearthspace-apps-private.service",
            "--property=After=hearthspace-apps-private.service",
            "--property=TimeoutStopSec=1",
            "--expand-environment=no",
        ]
    );
    assert_eq!(&manager_args[4..10], &args[..6]);
    assert_eq!(&manager_args[10..13], ["--", "/usr/bin/env", "--"]);
    assert_eq!(&manager_args[15..], &args[scope.command_start..]);
    let manager_env = command.get_envs().collect::<Vec<_>>();
    assert_eq!(
        manager_env.len(),
        2,
        "do not import session values into the manager"
    );
    let host_address = format!(
        "unix:path={}/host%3D%20space%25/bus",
        fixture.root.display()
    );
    assert!(manager_env.contains(&(
        OsStr::new("XDG_RUNTIME_DIR"),
        Some(fixture.host.as_os_str())
    )));
    assert!(manager_env.contains(&(
        OsStr::new("DBUS_SESSION_BUS_ADDRESS"),
        Some(OsStr::new(&host_address))
    )));

    // The manager is the external boundary. Run its actual shell command to
    // check that the env wrapper restores private values and preserves bytes.
    let mut shell = Command::new(manager_args[11]);
    shell.env_clear().args(&manager_args[12..]);
    for (key, value) in manager_env {
        shell.env(key, value.unwrap());
    }
    let output = shell.output().unwrap();
    assert!(output.status.success());
    let mut expected = Vec::new();
    for value in [fixture.runtime.as_os_str(), &fixture.bus]
        .into_iter()
        .chain(
            args[scope.command_start + 4..]
                .iter()
                .map(OsString::as_os_str),
        )
    {
        expected.extend_from_slice(value.as_bytes());
        expected.push(0);
    }
    assert_eq!(output.stdout, expected);
}

#[test]
fn a_private_runtime_and_its_matching_active_owner_are_required() {
    let fixture = Fixture::new();
    assert!(
        Session::load(
            fixture.host.clone(),
            fixture.host.clone(),
            fixture.bus.clone()
        )
        .is_err()
    );
    assert!(Session::load("relative".into(), fixture.host.clone(), fixture.bus.clone()).is_err());
    assert!(
        Session::load(
            fixture.runtime.clone(),
            fixture.host.clone(),
            "tcp:host=host".into()
        )
        .is_err()
    );
    fs::write(
        fixture.runtime.join("app-scope-owner"),
        "hearthspace-apps-other.service",
    )
    .unwrap();
    assert!(fixture.session().is_err());
}

#[test]
fn a_missing_guard_prevents_scope_registration() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.runtime.join(identity::READY_FILE)).unwrap();
    assert!(fixture.session().is_err());
    assert!(!fixture.record().exists());
}

#[test]
fn shutdown_after_registration_prevents_starting_the_scope() {
    let fixture = Fixture::new();
    let session = fixture.session().unwrap();
    let args = invocation(&["/bin/sh"]);
    let scope = scope_invocation(&args).unwrap();
    session.record(scope.unit).unwrap();
    fs::write(fixture.runtime.join(identity::CLOSING_FILE), b"").unwrap();
    assert!(session.command(&args, &scope).is_err());
    assert!(
        fixture.record().is_file(),
        "cleanup can still discover the recorded scope"
    );
}

#[test]
fn owner_change_after_registration_prevents_starting_the_scope() {
    let fixture = Fixture::new();
    let session = fixture.session().unwrap();
    let args = invocation(&["/bin/sh"]);
    let scope = scope_invocation(&args).unwrap();
    session.record(scope.unit).unwrap();
    fs::write(
        fixture.runtime.join("app-scope-owner"),
        "hearthspace-apps-other.service",
    )
    .unwrap();
    assert!(session.command(&args, &scope).is_err());
}

#[test]
fn existing_records_and_symlinks_cannot_be_overwritten() {
    let fixture = Fixture::new();
    let session = fixture.session().unwrap();
    session.record(UNIT).unwrap();
    assert_eq!(
        session.record(UNIT).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    fs::remove_file(fixture.record()).unwrap();
    let outside = fixture.root.join("outside");
    fs::write(&outside, b"untouched").unwrap();
    symlink(&outside, fixture.record()).unwrap();
    assert!(session.record(UNIT).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"untouched");
}

#[test]
fn an_app_scope_directory_symlink_cannot_redirect_registration() {
    let fixture = Fixture::new();
    let session = fixture.session().unwrap();
    symlink(&fixture.host, fixture.runtime.join("app-scopes")).unwrap();
    assert!(session.record(UNIT).is_err());
    assert!(!fixture.host.join(UNIT).exists());
}

#[test]
fn ownership_and_closing_marker_symlinks_fail_closed() {
    let fixture = Fixture::new();
    let owner = fixture.runtime.join("app-scope-owner");
    let outside = fixture.root.join("owner");
    fs::rename(&owner, &outside).unwrap();
    symlink(&outside, &owner).unwrap();
    assert!(fixture.session().is_err());
    fs::remove_file(&owner).unwrap();
    fs::rename(&outside, &owner).unwrap();
    symlink(
        fixture.root.join("missing"),
        fixture.runtime.join(identity::CLOSING_FILE),
    )
    .unwrap();
    assert!(fixture.session().is_err());
}
