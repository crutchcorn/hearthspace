use super::*;
use hearthspace_ipc::{AccentColor, BackgroundMode, ColorMode};

#[test]
fn versioned_toml_round_trips_quotes_hashes_unicode_and_newline_paths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let mut settings = Settings {
        background_image_path: Some("/tmp/虹 #1/\"image\"\nnext.png".into()),
        ..Settings::default()
    };
    settings.monitors[0].name = "Desk #1 \"虹\"\nDisplay".into();
    settings.monitors[0].id = "DP #1".into();
    save_settings_to_path(settings.clone(), &path).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    let document: toml::Table = toml::from_str(&contents).unwrap();
    assert_eq!(document["version"].as_integer(), Some(1));
    assert_eq!(load_settings_from_path(&path).unwrap(), settings);
}

#[test]
fn legacy_reads_preserve_values_without_rewriting_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let contents = "test = true\nbackground_moves_with_canvas = false\n\
        background_image_path = /tmp/虹 #1.png\ncolor_mode = dark\naccent_color = teal\n\
        [[monitor]]\nid = DP-3\nname = Desk #3\nscale = 2\nx = -900\ny = 80\n";
    fs::write(&path, contents).unwrap();

    let settings = load_settings_from_path(&path).unwrap();
    assert!(settings.test);
    assert_eq!(settings.background_mode, BackgroundMode::Cover);
    assert_eq!(
        settings.background_image_path.as_deref(),
        Some("/tmp/虹 #1.png")
    );
    assert_eq!(settings.appearance.color_mode, ColorMode::Dark);
    assert_eq!(settings.appearance.accent_color, AccentColor::Teal);
    assert_eq!(settings.monitors[0].name, "Desk #3");
    assert_eq!(settings.monitors[0].width, 1920);
    assert_eq!(fs::read_to_string(&path).unwrap(), contents);
}

#[test]
fn unversioned_quoted_paths_are_parsed_as_toml_strings() {
    let settings = document::parse(
        r##"
background_image_path = "/tmp/\"虹\" #1\nnext.png"
background_mode = "stretch"
[[monitor]]
id = "DP-3"
name = "Desk #3"
"##,
    )
    .unwrap();
    assert_eq!(
        settings.background_image_path.as_deref(),
        Some("/tmp/\"虹\" #1\nnext.png")
    );
    assert_eq!(settings.monitors[0].name, "Desk #3");
    assert_eq!(settings.background_mode, BackgroundMode::Stretch);
}

#[test]
fn missing_preferences_keep_defaults_and_an_explicit_empty_layout_stays_empty() {
    assert_eq!(
        document::parse("version = 1\n").unwrap(),
        Settings::default()
    );
    assert_eq!(document::parse("").unwrap(), Settings::default());
    let settings = document::parse("version = 1\nmonitor = []\n").unwrap();
    assert!(settings.monitors.is_empty());
}

#[test]
fn malformed_or_unknown_versions_and_bad_legacy_values_are_errors() {
    for contents in [
        "version = 2\n",
        "version = \"one\"\n",
        "version = nope\n",
        "version = 1\nbackground_mode = cover\n",
        "version = 1\nbackground_image_path = \"unterminated\n",
        "test = perhaps\n",
        "test = true\ntest = false\n",
        "background_image_path = \"unterminated\n",
        "unexpected = discarded\n",
        "[[monitor]]\nname = missing identity\n",
        "[[monitor]]\nid = DP-1\nscale = 0\n",
    ] {
        assert!(document::parse(contents).is_err(), "accepted {contents:?}");
    }
}

#[test]
fn invalid_save_does_not_replace_existing_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    save_settings_to_path(Settings::default(), &path).unwrap();
    let original = fs::read(&path).unwrap();
    let mut invalid = Settings::default();
    invalid.monitors[1].height = -1;
    assert!(save_settings_to_path(invalid, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
