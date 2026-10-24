use super::tests::catalog;

#[test]
fn compact_desktop_id_matches_without_using_the_display_name_or_title() {
    let catalog = catalog(&[(
        "Pepper Grinder.desktop",
        "[Desktop Entry]\nType=Application\nName=Pepper Grinder\nExec=steam steam://rungameid/2076580\nIcon=steam_icon_2076580\n",
    )]);

    assert_eq!(
        catalog.app_for_window("PepperGrinder").unwrap().id,
        "Pepper Grinder.desktop"
    );
    assert_eq!(
        catalog.app_for_window("peppergrinder.desktop").unwrap().id,
        "Pepper Grinder.desktop"
    );
    assert!(
        catalog
            .app_for_window("Update Required - Pepper Grinder")
            .is_none()
    );
    assert!(catalog.app_for_window("Pepper-Grinder").is_none());
}

#[test]
fn explicit_window_metadata_wins_over_a_compacted_desktop_id() {
    let catalog = catalog(&[
        (
            "Pepper Grinder.desktop",
            "[Desktop Entry]\nType=Application\nName=Game\nExec=steam steam://rungameid/2076580\n",
        ),
        (
            "com.example.Editor.desktop",
            "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor\nStartupWMClass=PepperGrinder\n",
        ),
    ]);

    assert_eq!(
        catalog.app_for_window("PepperGrinder").unwrap().id,
        "com.example.Editor.desktop"
    );
}

#[test]
fn ambiguous_compacted_ids_do_not_fall_through_to_an_executable() {
    let catalog = catalog(&[
        (
            "Pepper Grinder.desktop",
            "[Desktop Entry]\nType=Application\nName=First game\nExec=steam steam://rungameid/1\n",
        ),
        (
            "Pepper  Grinder.desktop",
            "[Desktop Entry]\nType=Application\nName=Second game\nExec=steam steam://rungameid/2\n",
        ),
        (
            "other.desktop",
            "[Desktop Entry]\nType=Application\nName=Other\nExec=PepperGrinder\n",
        ),
    ]);

    assert!(catalog.app_for_window("PepperGrinder").is_none());
}

#[test]
fn compact_identity_preserves_punctuation_and_literal_desktop_suffix() {
    let catalog = catalog(&[(
        "org.example.My Editor.desktop.desktop",
        "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor-helper\n",
    )]);

    assert_eq!(
        catalog
            .app_for_window("org.example.MyEditor.desktop")
            .unwrap()
            .id,
        "org.example.My Editor.desktop.desktop"
    );
    assert!(catalog.app_for_window("org.example.MyEditor").is_none());
    assert!(
        catalog
            .app_for_window("orgexampleMyEditordesktop")
            .is_none()
    );
    assert!(catalog.app_for_window("Editor").is_none());
    assert!(catalog.app_for_window("MyEditor").is_none());
}
