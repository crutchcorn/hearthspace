use super::*;

#[test]
fn parses_ipc_requests() {
    assert_eq!(parse_settings_request("get\n"), Ok(SettingsRequest::Get));
    assert_eq!(
        parse_settings_request("set test true\n"),
        Ok(SettingsRequest::SetTest(true))
    );
    assert_eq!(
        parse_settings_request("set background-mode cover\n"),
        Ok(SettingsRequest::SetBackgroundMode(BackgroundMode::Cover))
    );
    assert_eq!(
        parse_settings_request("set background-moves-with-canvas false\n"),
        Ok(SettingsRequest::SetBackgroundMode(BackgroundMode::Cover))
    );
    assert_eq!(
        parse_settings_request("set background-image-path %2Ftmp%2Fwallpaper.png\n"),
        Ok(SettingsRequest::SetBackgroundImagePath(Some(
            "/tmp/wallpaper.png".to_string()
        )))
    );
    assert_eq!(
        parse_settings_request("set background-image-path default\n"),
        Ok(SettingsRequest::SetBackgroundImagePath(None))
    );
    assert_eq!(
        parse_settings_request("set monitor primary 120 -40\n"),
        Ok(SettingsRequest::SetMonitorPosition {
            id: "primary".to_string(),
            x: 120,
            y: -40,
        })
    );
    assert_eq!(
        parse_settings_request("set monitor-scale primary 2\n"),
        Ok(SettingsRequest::SetMonitorScale {
            id: "primary".to_string(),
            scale: 2,
        })
    );
}

#[test]
fn parses_ipc_response() {
    assert_eq!(
            parse_settings_response(
                "ok test true background-mode contains background-image-path %2Ftmp%2Fwallpaper.png monitor primary 1920 1080 2 120 -40 Primary monitor secondary 1280 720 1 2040 0 Secondary%20Display\n"
            )
            .unwrap(),
            Settings {
                test: true,
                appearance: Appearance::default(),
                background_mode: BackgroundMode::Contains,
                background_image_path: Some("/tmp/wallpaper.png".to_string()),
                monitors: vec![
                    MonitorConfig {
                        id: "primary".to_string(),
                        name: "Primary".to_string(),
                        width: 1920,
                        height: 1080,
                        scale: 2,
                        x: 120,
                        y: -40,
                    },
                    MonitorConfig {
                        id: "secondary".to_string(),
                        name: "Secondary Display".to_string(),
                        width: 1280,
                        height: 720,
                        scale: 1,
                        x: 2040,
                        y: 0,
                    },
                ],
            }
        );
}

#[test]
fn parses_legacy_ipc_response_with_background_moves_with_canvas() {
    assert_eq!(
            parse_settings_response(
                "ok test true background-moves-with-canvas false background-image-path %2Ftmp%2Fwallpaper.png monitor primary 1920 1080 2 120 -40 Primary\n"
            )
            .unwrap()
            .background_mode,
            BackgroundMode::Cover
        );
}

#[test]
fn parses_legacy_ipc_response_with_default_background_setting() {
    assert_eq!(
        parse_settings_response("ok test true monitor primary 1920 1080 1 0 0 Primary\n")
            .unwrap()
            .background_mode,
        Settings::default().background_mode
    );
}

#[test]
fn appearance_choices_round_trip_without_losing_other_settings() {
    for color_mode in ColorMode::ALL {
        for accent_color in AccentColor::ALL {
            let settings = Settings {
                appearance: Appearance {
                    color_mode,
                    accent_color,
                },
                test: true,
                background_mode: BackgroundMode::Cover,
                background_image_path: Some("/tmp/my photo.png".into()),
                ..Settings::default()
            };
            assert_eq!(
                parse_settings_response(&settings_response(settings.clone())).unwrap(),
                settings,
            );
            assert_eq!(
                parse_settings_request(&format!("set color-mode {color_mode}")),
                Ok(SettingsRequest::SetColorMode(color_mode)),
            );
            assert_eq!(
                parse_settings_request(&format!("set accent-color {accent_color}")),
                Ok(SettingsRequest::SetAccentColor(accent_color)),
            );
        }
    }
    assert_eq!(
        parse_settings_response("ok test false\n")
            .unwrap()
            .appearance,
        Appearance::default(),
    );
    for invalid in [
        "set color-mode auto",
        "set color-mode",
        "set accent-color pink",
    ] {
        assert!(parse_settings_request(invalid).is_err());
    }
    for invalid in [
        "ok test false color-mode auto",
        "ok test false accent-color ultraviolet",
        "ok test false accent-color",
    ] {
        assert!(parse_settings_response(invalid).is_err());
    }
}
