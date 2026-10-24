use std::{env, fs, path::Path, process::Command};

use hearthspace_desktop::{app_catalog::AppCatalog, app_icon, artwork::IconSource};

const CHILD_MODE: &str = "HEARTHSPACE_TEST_APP_ICONS";
const UPDATER: [u8; 4] = [240, 100, 20, 255];
const STEAM: [u8; 4] = [20, 40, 70, 255];
const PEPPER: [u8; 4] = [250, 160, 30, 255];
const CLIENT_ALIAS: &str = "hearthspace-fixture-snap-client-4a17f9";
const GAME: &str = "Z.A.T.O.  I Love the World and Everything In It.desktop";
const HASH: &str = "279871be81ffe844e0ac24a1afea74af33126822";

#[test]
fn configured_theme_steam_games_and_exported_snap_artwork_reach_desktop_icons() {
    let fixture = tempfile::tempdir().unwrap();
    let share = fixture.path().join("share");
    let schemas = share.join("glib-2.0/schemas");
    let applications = share.join("applications");
    let theme = share.join("icons/HearthspaceTest");
    fs::create_dir_all(&schemas).unwrap();
    fs::create_dir_all(&applications).unwrap();
    fs::create_dir_all(theme.join("48x48/apps")).unwrap();
    fs::write(
        schemas.join("org.gnome.desktop.interface.gschema.xml"),
        r#"
        <schemalist><schema id="org.gnome.desktop.interface" path="/org/gnome/desktop/interface/">
        <key name="icon-theme" type="s"><default>'HearthspaceTest'</default></key>
        </schema></schemalist>
    "#,
    )
    .unwrap();
    // GIO's schema compiler is supplied by the GLib development dependencies.
    let output = Command::new("glib-compile-schemas")
        .arg(&schemas)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(theme.join("index.theme"), "[Icon Theme]\nName=HearthspaceTest\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\nContext=Applications\n").unwrap();
    write_png(&theme.join("48x48/apps/software-updater.png"), UPDATER);
    std::os::unix::fs::symlink(
        "software-updater.png",
        theme.join("48x48/apps/system-software-update.png"),
    )
    .unwrap();
    write_png(&theme.join("48x48/apps/steam.png"), STEAM);
    write_png(&theme.join("48x48/apps/steam_icon_2076580.png"), PEPPER);
    let steam_icon = fixture
        .path()
        .join("snap/steam/current/usr/share/icons/hicolor/256x256/apps/steam.png");
    write_png(&steam_icon, STEAM);
    fs::write(applications.join("update-manager.desktop"), "[Desktop Entry]\nType=Application\nName=Software Updater\nExec=/usr/bin/update-manager\nIcon=system-software-update\n").unwrap();
    // The installed launcher has no StartupWMClass; the native client reports
    // PepperGrinder for both class and instance, with an empty _NET_WM_ICON.
    fs::write(applications.join("Pepper Grinder.desktop"), "[Desktop Entry]\nName=Pepper Grinder\nComment=Play this game on Steam\nExec=steam steam://rungameid/2076580\nIcon=steam_icon_2076580\nTerminal=false\nType=Application\nCategories=Game;\n").unwrap();
    // Use a unique alias: freedesktop-icons also searches /usr/share/pixmaps,
    // even with isolated XDG paths, so a host steam.png could mask this fallback.
    // Keep the provider outside applications until the child installs it below.
    fs::write(share.join("snap-client.desktop"), format!("[Desktop Entry]\nType=Application\nName=Snap Client\nExec=/snap/bin/fixture-client %U\nIcon={}\nX-SnapAppName={CLIENT_ALIAS}\n", steam_icon.display())).unwrap();
    fs::write(applications.join("steam_steam.desktop"), "[Desktop Entry]\nType=Application\nName=Steam\nExec=/snap/bin/steam %U\nIcon=steam\nX-SnapAppName=steam\n").unwrap();
    fs::write(applications.join(GAME), "[Desktop Entry]\nType=Application\nName=Z.A.T.O. // I Love the World and Everything In It\nExec=steam steam://rungameid/4122860\nIcon=steam\nStartupWMClass=ZatoGame\n").unwrap();
    fs::write(applications.join("Second Game.desktop"), "[Desktop Entry]\nType=Application\nName=Second Game\nExec=steam steam://rungameid/9876543210987654321\nIcon=steam\nStartupWMClass=SecondGame\n").unwrap();
    let home = fixture.path().join("home");
    write_jpeg(
        &home.join(format!(
            "snap/steam/common/.local/share/Steam/appcache/librarycache/4122860/{HASH}.jpg"
        )),
        [220, 30, 60],
    );
    write_jpeg(&home.join(format!(".var/app/com.valvesoftware.Steam/.local/share/Steam/appcache/librarycache/9876543210987654321/{HASH}.jpg")), [40, 210, 90]);
    // Both the explicit and derived game-specific name must beat cached artwork.
    write_jpeg(
        &home.join(format!(
            ".local/share/Steam/appcache/librarycache/2076580/{HASH}.jpg"
        )),
        [120, 20, 200],
    );

    // Isolate GSettings and the icon library's process-wide XDG caches from
    // other tests and the machine's installed themes and application catalog.
    let output = Command::new(env::current_exe().unwrap())
        .args(["--exact", "resolve_installed_icons_in_child", "--nocapture"])
        .env_clear()
        .env(CHILD_MODE, "1")
        .env("HOME", &home)
        .env("XDG_DATA_HOME", fixture.path().join("home/share"))
        .env("XDG_DATA_DIRS", &share)
        .env("GSETTINGS_SCHEMA_DIR", &schemas)
        .env("GSETTINGS_BACKEND", "memory")
        .current_dir(fixture.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn resolve_installed_icons_in_child() {
    if env::var_os(CHILD_MODE).is_none() {
        return;
    }
    let catalog = AppCatalog::load();
    let updater = catalog.app_by_id("update-manager.desktop").unwrap();
    let updater_artwork = app_icon::resolve_app(updater)
        .expect("the configured icon theme supplies Software Updater's artwork");
    assert_eq!(&updater_artwork.rgba[..4], &UPDATER);
    assert_eq!(
        app_icon::resolve_window(&catalog, "update-manager"),
        Some(updater_artwork)
    );

    let pepper = catalog.app_by_id("Pepper Grinder.desktop").unwrap();
    let pepper_artwork = app_icon::resolve_app(pepper)
        .expect("the desktop shortcut resolves the installed game icon");
    assert_eq!(&pepper_artwork.rgba[..4], &PEPPER);
    assert_eq!(
        app_icon::resolve_window(&catalog, "PepperGrinder"),
        Some(pepper_artwork.clone())
    );
    let mut generic_pepper = pepper.clone();
    generic_pepper.icon = Some("steam".into());
    assert_eq!(app_icon::resolve_app(&generic_pepper), Some(pepper_artwork));

    distinct_game_artwork_and_custom_override(&catalog);
    invalid_game_cache_falls_back_to_metadata(&catalog);
    fresh_catalog_alias_resolves_after_provider_install();
}

fn distinct_game_artwork_and_custom_override(catalog: &AppCatalog) {
    let game = catalog.app_by_id(GAME).unwrap();
    let second = catalog.app_by_id("Second Game.desktop").unwrap();
    let client = catalog.app_by_id("steam_steam.desktop").unwrap();
    let game_artwork = app_icon::resolve_app(game).expect("Snap's unique cached game JPEG");
    let second_artwork = app_icon::resolve_app(second).expect("Flatpak's unique cached game JPEG");
    let client_artwork = app_icon::resolve_app(client).unwrap();
    assert_eq!((game_artwork.width, game_artwork.height), (48, 48));
    assert_eq!((second_artwork.width, second_artwork.height), (48, 48));
    assert_ne!(game_artwork, second_artwork);
    assert_ne!(game_artwork, client_artwork);
    assert_ne!(second_artwork, client_artwork);
    assert_eq!(&client_artwork.rgba[..4], &STEAM);
    assert_eq!(
        app_icon::resolve_window(catalog, "ZatoGame"),
        Some(game_artwork)
    );
    assert_eq!(
        app_icon::resolve_window(catalog, "SecondGame"),
        Some(second_artwork)
    );
    assert_eq!(
        app_icon::resolve_window(catalog, "Steam"),
        Some(client_artwork.clone())
    );

    let mut custom = game.clone();
    custom.icon = Some("system-software-update".into());
    assert_eq!(&app_icon::resolve_app(&custom).unwrap().rgba[..4], &UPDATER);
    let path = env::current_dir().unwrap().join("custom-game.png");
    write_png(&path, PEPPER);
    custom.icon = Some(path.to_string_lossy().into_owned());
    assert_eq!(&app_icon::resolve_app(&custom).unwrap().rgba[..4], &PEPPER);

    // A Steam-looking title and URI argument on another executable do not
    // grant game identity. Named/tray consumers also retain the client icon.
    custom.icon = Some("steam".into());
    custom.exec = "other steam://rungameid/4122860".into();
    assert_eq!(app_icon::resolve_app(&custom), Some(client_artwork.clone()));
    assert_eq!(
        app_icon::resolve(&IconSource {
            name: Some("steam".into()),
            ..Default::default()
        }),
        Some(client_artwork)
    );
}

fn invalid_game_cache_falls_back_to_metadata(catalog: &AppCatalog) {
    let game = catalog.app_by_id(GAME).unwrap();
    let client = app_icon::resolve_app(catalog.app_by_id("steam_steam.desktop").unwrap());
    let directory = std::path::PathBuf::from(env::var_os("HOME").unwrap())
        .join("snap/steam/common/.local/share/Steam/appcache/librarycache/4122860");
    let path = directory.join(format!("{HASH}.jpg"));
    let other = directory.join("0123456789abcdef0123456789abcdef01234567.jpg");
    write_jpeg(&other, [0, 0, 200]);
    assert_eq!(app_icon::resolve_app(game), client, "ambiguous cache");
    fs::remove_file(other).unwrap();
    fs::write(&path, b"not a jpeg").unwrap();
    assert_eq!(app_icon::resolve_app(game), client, "malformed JPEG");
    fs::File::create(&path)
        .unwrap()
        .set_len(2 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(app_icon::resolve_app(game), client, "oversized file");
    fs::remove_file(path).unwrap();
    assert_eq!(app_icon::resolve_app(game), client, "missing cache");
}

fn fresh_catalog_alias_resolves_after_provider_install() {
    // This regression is independent of Steam game's Icon=steam resolution.
    let icon = IconSource {
        name: Some(CLIENT_ALIAS.into()),
        ..Default::default()
    };
    assert!(app_icon::resolve(&icon).is_none());
    let share = std::path::PathBuf::from(env::var_os("XDG_DATA_DIRS").unwrap());
    fs::rename(
        share.join("snap-client.desktop"),
        share.join("applications/snap-client.desktop"),
    )
    .unwrap();
    assert!(AppCatalog::load().app_for_window(CLIENT_ALIAS).is_some());
    let artwork = app_icon::resolve(&icon)
        .expect("a fresh catalog sees the newly exported Snap desktop entry");
    assert_eq!(&artwork.rgba[..4], &STEAM);
    assert_eq!(
        app_icon::resolve_window(&AppCatalog::load(), CLIENT_ALIAS),
        Some(artwork)
    );
}

fn write_jpeg(path: &Path, color: [u8; 3]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(32, 32, image::Rgb(color))
        .save(path)
        .unwrap();
}

fn write_png(path: &Path, color: [u8; 4]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(48, 48, image::Rgba(color))
        .save(path)
        .unwrap();
}
