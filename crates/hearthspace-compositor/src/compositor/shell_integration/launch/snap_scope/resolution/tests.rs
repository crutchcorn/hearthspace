use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    root: tempfile::TempDir,
    snap_bin: PathBuf,
    ordinary: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let snap_bin = root.path().join("snap/bin");
        let ordinary = root.path().join("ordinary");
        fs::create_dir_all(&snap_bin).unwrap();
        fs::create_dir(&ordinary).unwrap();
        // Like real /snap/bin/steam, the exported name points to a generic
        // launcher. Canonicalizing the final component loses the app identity.
        symlink("/bin/true", snap_bin.join("steam")).unwrap();
        Self {
            root,
            snap_bin,
            ordinary,
        }
    }

    fn path(&self) -> std::ffi::OsString {
        env::join_paths([&self.ordinary, &self.snap_bin]).unwrap()
    }

    fn resolve(&self, command: &mut [String]) -> Result<Option<String>, String> {
        resolve_with(
            None,
            command,
            self.root.path(),
            &self.path(),
            &self.snap_bin,
        )
    }

    fn shadow(&self, mode: u32) {
        let path = self.ordinary.join("steam");
        fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
}

fn shortcut() -> Vec<String> {
    vec!["steam".into(), "steam://rungameid/2076580".into()]
}

#[test]
fn steam_game_shortcut_without_snap_metadata_gets_an_owned_snap_scope() {
    let fixture = Fixture::new();
    let applications = fixture.root.path().join("applications");
    fs::create_dir(&applications).unwrap();
    fs::write(applications.join("Pepper Grinder.desktop"),
        "[Desktop Entry]\nType=Application\nName=Pepper Grinder\nExec=steam steam://rungameid/2076580\nTerminal=false\n").unwrap();
    let catalog = hearthspace_shell::app_catalog::AppCatalog::load_from_data_dirs(vec![
        fixture.root.path().into(),
    ]);
    let app = catalog.app_by_id("Pepper Grinder.desktop").unwrap();
    assert_eq!(app.snap_instance_name, None);
    let mut argv = app.launch_argv().unwrap();
    let instance = fixture
        .resolve(&mut argv)
        .unwrap()
        .expect("Steam shortcut must receive Snap tracking");
    assert_eq!(instance, "steam");
    assert_eq!(argv, ["/snap/bin/steam", "steam://rungameid/2076580"]);
    let wrapped = super::super::wrap_with(
        &instance,
        argv.clone(),
        "hearthspace-apps-test.service",
        "/private",
        "/run/user/1000",
        "unix:path=/private/bus",
        "01234567-89ab-cdef-0123-456789abcdef",
    )
    .unwrap();
    assert!(
        wrapped
            .contains(&"--unit=snap.steam.steam-01234567-89ab-cdef-0123-456789abcdef.scope".into())
    );
    assert!(wrapped.contains(&"--property=BindsTo=hearthspace-apps-test.service".into()));
    assert_eq!(&wrapped[wrapped.len() - argv.len()..], argv);
}

#[test]
fn an_executable_earlier_in_path_keeps_the_shortcut_ordinary() {
    let fixture = Fixture::new();
    fixture.shadow(0o700);
    let mut argv = shortcut();
    assert_eq!(fixture.resolve(&mut argv).unwrap(), None);
    assert_eq!(argv, shortcut());
}

#[test]
fn nonexecutable_shadow_is_skipped() {
    let fixture = Fixture::new();
    fixture.shadow(0o600);
    let mut argv = shortcut();
    assert_eq!(
        fixture.resolve(&mut argv).unwrap().as_deref(),
        Some("steam")
    );
}

#[test]
fn desktop_working_directory_controls_relative_and_empty_path_entries() {
    let fixture = Fixture::new();
    let mut argv = shortcut();
    assert_eq!(
        resolve_with(
            None,
            &mut argv,
            fixture.root.path(),
            OsStr::new("snap/bin"),
            &fixture.snap_bin
        )
        .unwrap()
        .as_deref(),
        Some("steam")
    );
    let mut argv = shortcut();
    assert_eq!(
        resolve_with(
            None,
            &mut argv,
            &fixture.snap_bin,
            OsStr::new(""),
            &fixture.snap_bin
        )
        .unwrap()
        .as_deref(),
        Some("steam")
    );
}

#[test]
fn explicit_executable_paths_do_not_fall_back_to_path_search() {
    let fixture = Fixture::new();
    fixture.shadow(0o700);
    for program in [
        fixture.snap_bin.join("steam"),
        PathBuf::from("snap/bin/steam"),
    ] {
        let mut argv = vec![program.to_str().unwrap().into()];
        assert_eq!(
            resolve_with(
                None,
                &mut argv,
                fixture.root.path(),
                OsStr::new("/missing"),
                &fixture.snap_bin
            )
            .unwrap()
            .as_deref(),
            Some("steam")
        );
        assert_eq!(argv, ["/snap/bin/steam"]);
    }
    let mut argv = vec![fixture.ordinary.join("steam").to_str().unwrap().into()];
    let original = argv.clone();
    assert_eq!(fixture.resolve(&mut argv).unwrap(), None);
    assert_eq!(argv, original);
}

#[test]
fn trusted_env_path_assignments_control_lookup_without_expanding_values() {
    let fixture = Fixture::new();
    fixture.shadow(0o700);
    let mut argv = vec![
        "/usr/bin/env".into(),
        "--".into(),
        format!("PATH={}", fixture.snap_bin.display()),
        "steam".into(),
        "literal $PATH;argument".into(),
    ];
    assert_eq!(
        fixture.resolve(&mut argv).unwrap().as_deref(),
        Some("steam")
    );
    assert_eq!(argv[3], "/snap/bin/steam");
    assert_eq!(argv[4], "literal $PATH;argument");
    assert_eq!(
        super::super::security_tag("steam", &argv).unwrap(),
        "snap.steam.steam"
    );
    let mut argv = vec!["/usr/bin/env".into(), "PATH=$PATH".into(), "steam".into()];
    assert_eq!(fixture.resolve(&mut argv).unwrap(), None);
    assert_eq!(argv[2], "steam");
}

#[test]
fn a_shadowed_env_or_unsupported_env_option_is_not_reinterpreted() {
    let fixture = Fixture::new();
    symlink("/bin/true", fixture.ordinary.join("env")).unwrap();
    let mut argv = vec!["env".into(), "steam".into()];
    assert_eq!(fixture.resolve(&mut argv).unwrap(), None);
    assert_eq!(argv, ["env", "steam"]);
    let mut argv = vec![
        "/usr/bin/env".into(),
        "-C".into(),
        "/snap/bin".into(),
        "steam".into(),
    ];
    assert_eq!(fixture.resolve(&mut argv).unwrap(), None);
}

#[test]
fn a_system_env_alias_is_normalized_only_after_snap_identity_validation() {
    let fixture = Fixture::new();
    let original = vec![
        "/bin/env".into(),
        "steam".into(),
        "steam://rungameid/2076580".into(),
    ];
    let mut argv = original.clone();
    assert!(
        resolve_with(
            Some("firefox"),
            &mut argv,
            fixture.root.path(),
            &fixture.path(),
            &fixture.snap_bin
        )
        .is_err()
    );
    assert_eq!(argv, original);
    assert_eq!(
        fixture.resolve(&mut argv).unwrap().as_deref(),
        Some("steam")
    );
    assert_eq!(
        argv,
        [
            "/usr/bin/env",
            "/snap/bin/steam",
            "steam://rungameid/2076580"
        ]
    );
    let wrapped = super::super::wrap_with(
        "steam",
        argv.clone(),
        "hearthspace-apps-test.service",
        "/private",
        "/run/user/1000",
        "unix:path=/private/bus",
        "01234567-89ab-cdef-0123-456789abcdef",
    )
    .unwrap();
    assert_eq!(&wrapped[wrapped.len() - argv.len()..], argv);
}

#[test]
fn mismatched_snap_metadata_is_rejected_without_mutating_the_command() {
    let fixture = Fixture::new();
    let mut argv = shortcut();
    assert!(
        resolve_with(
            Some("firefox"),
            &mut argv,
            fixture.root.path(),
            &fixture.path(),
            &fixture.snap_bin
        )
        .is_err()
    );
    assert_eq!(argv, shortcut());
}

#[test]
fn an_alias_of_the_export_directory_keeps_the_exported_application_name() {
    let fixture = Fixture::new();
    let alias = fixture.root.path().join("alias");
    symlink(&fixture.snap_bin, &alias).unwrap();
    let mut argv = shortcut();
    assert_eq!(
        resolve_with(
            None,
            &mut argv,
            fixture.root.path(),
            alias.as_os_str(),
            &fixture.snap_bin
        )
        .unwrap()
        .as_deref(),
        Some("steam")
    );
    assert_eq!(argv[0], "/snap/bin/steam");
}
