use super::*;

fn catalog(entries: &[(&str, &str)]) -> AppCatalog {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("applications")).unwrap();
    for (id, contents) in entries {
        std::fs::write(directory.path().join("applications").join(id), contents).unwrap();
    }
    AppCatalog::load_from_data_dirs(vec![directory.path().to_owned()])
}

#[test]
fn snap_windows_match_exported_application_identity() {
    let catalog = catalog(&[(
        "vlc_vlc.desktop",
        "[Desktop Entry]\nType=Application\nName=VLC media player\nExec=/snap/bin/vlc --started-from-file %U\nIcon=/snap/vlc/current/vlc.png\nX-SnapInstanceName=vlc\nX-SnapAppName=vlc\nX-SnapCommonID=org.videolan.vlc\n",
    )]);

    assert_eq!(catalog.app_for_window("Vlc").unwrap().id, "vlc_vlc.desktop");
    assert_eq!(
        catalog
            .app_for_window("org.videolan.vlc")
            .unwrap()
            .icon
            .as_deref(),
        Some("/snap/vlc/current/vlc.png")
    );
}

#[test]
fn startup_class_works_with_unrelated_desktop_and_executable_names() {
    let catalog = catalog(&[(
        "com.example.Editor.desktop",
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=/opt/editor/start\nStartupWMClass=EditorWindow\nIcon=editor-art\n",
    )]);

    assert_eq!(
        catalog
            .app_for_window("EditorWindow")
            .unwrap()
            .icon
            .as_deref(),
        Some("editor-art")
    );
}

#[test]
fn desktop_suffix_can_be_part_of_the_application_identity() {
    let catalog = catalog(&[(
        "ai.opencode.desktop.desktop",
        "[Desktop Entry]\nType=Application\nName=OpenCode\nExec=/opt/OpenCode/ai.opencode.desktop %U\nIcon=ai.opencode.desktop\nStartupWMClass=ai.opencode.desktop\n",
    )]);

    assert_eq!(
        catalog.app_for_window("ai.opencode.desktop").unwrap().id,
        "ai.opencode.desktop.desktop"
    );
    assert_eq!(
        catalog
            .app_for_window("ai.opencode.desktop.desktop")
            .unwrap()
            .id,
        "ai.opencode.desktop.desktop"
    );
    assert!(catalog.app_for_window("ai.opencode").is_none());
}

#[test]
fn literal_application_identity_wins_over_a_desktop_filename_alias() {
    let catalog = catalog(&[
        (
            "org.example.desktop.desktop",
            "[Desktop Entry]\nType=Application\nName=Desktop\nExec=example-desktop\n",
        ),
        (
            "org.example.desktop",
            "[Desktop Entry]\nType=Application\nName=Example\nExec=example\n",
        ),
    ]);

    assert_eq!(
        catalog.app_for_window("org.example.desktop").unwrap().id,
        "org.example.desktop.desktop"
    );
}

#[test]
fn startup_class_retains_a_literal_desktop_suffix() {
    let catalog = catalog(&[(
        "com.example.Editor.desktop",
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=/opt/editor/start\nStartupWMClass=editor.desktop\n",
    )]);

    assert_eq!(
        catalog.app_for_window("editor.desktop").unwrap().id,
        "com.example.Editor.desktop"
    );
}

#[test]
fn executable_identity_retains_a_literal_desktop_suffix() {
    let catalog = catalog(&[(
        "com.example.Editor.desktop",
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=/opt/editor/editor.desktop %U\n",
    )]);

    assert_eq!(
        catalog.app_for_window("editor.desktop").unwrap().id,
        "com.example.Editor.desktop"
    );
}

#[test]
fn desktop_identity_wins_over_other_apps_startup_class() {
    let catalog = catalog(&[
        (
            "editor.desktop",
            "[Desktop Entry]\nType=Application\nName=Z editor\nExec=editor\nIcon=editor-art\n",
        ),
        (
            "other.desktop",
            "[Desktop Entry]\nType=Application\nName=A other\nExec=other\nStartupWMClass=editor\nIcon=other-art\n",
        ),
    ]);

    assert_eq!(
        catalog.app_for_window("editor").unwrap().id,
        "editor.desktop"
    );
    assert_eq!(
        catalog.app_for_window("Editor.desktop").unwrap().id,
        "editor.desktop"
    );
}

#[test]
fn unique_executable_basename_handles_launchers_without_window_metadata() {
    let catalog = catalog(&[(
        "packaged-player.desktop",
        "[Desktop Entry]\nType=Application\nName=Player\nExec=\"/opt/Media Player/bin/player\" %U\nIcon=player-art\n",
    )]);

    assert_eq!(
        catalog.app_for_window("Player").unwrap().id,
        "packaged-player.desktop"
    );
    assert!(catalog.app_for_window("Media Player").is_none());
    assert!(catalog.app_for_window("").is_none());
}

#[test]
fn shared_launcher_does_not_choose_an_arbitrary_game() {
    let catalog = catalog(&[
        (
            "first-game.desktop",
            "[Desktop Entry]\nType=Application\nName=First game\nExec=steam steam://rungameid/111\nIcon=first-game\n",
        ),
        (
            "second-game.desktop",
            "[Desktop Entry]\nType=Application\nName=Second game\nExec=steam steam://rungameid/222\nIcon=second-game\n",
        ),
    ]);

    assert!(catalog.app_for_window("Steam").is_none());
}

#[test]
fn snap_launcher_identity_wins_over_a_games_shared_executable() {
    let catalog = catalog(&[
        (
            "steam_steam.desktop",
            "[Desktop Entry]\nType=Application\nName=Steam\nExec=/snap/bin/steam %U\nX-SnapAppName=steam\nIcon=/snap/steam/current/usr/share/icons/hicolor/256x256/apps/steam.png\n",
        ),
        (
            "Z.A.T.O.  I Love the World and Everything In It.desktop",
            "[Desktop Entry]\nType=Application\nName=Z.A.T.O. // I Love the World and Everything In It\nExec=steam steam://rungameid/4122860\nIcon=steam\n",
        ),
    ]);

    assert_eq!(
        catalog.app_for_window("Steam").unwrap().id,
        "steam_steam.desktop"
    );
}

#[test]
fn ambiguous_snap_identity_does_not_fall_through_to_a_game_executable() {
    let catalog = catalog(&[
        (
            "steam_first.desktop",
            "[Desktop Entry]\nType=Application\nName=Steam first\nExec=/snap/bin/steam.first %U\nX-SnapAppName=steam\nIcon=first-steam\n",
        ),
        (
            "steam_second.desktop",
            "[Desktop Entry]\nType=Application\nName=Steam second\nExec=/snap/bin/steam.second %U\nX-SnapAppName=steam\nIcon=second-steam\n",
        ),
        (
            "game.desktop",
            "[Desktop Entry]\nType=Application\nName=Game\nExec=steam steam://rungameid/123\nIcon=game-art\n",
        ),
    ]);

    assert!(catalog.app_for_window("Steam").is_none());
}
