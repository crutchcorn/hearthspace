use std::{env, fs, path::Path, process::Command};

use hearthspace_shell::{app_catalog::AppCatalog, app_icon, tray::TrayIcon};

const CHILD_MODE: &str = "HEARTHSPACE_TEST_APP_ICONS";
const UPDATER: [u8; 4] = [240, 100, 20, 255];
const STEAM: [u8; 4] = [20, 40, 70, 255];
const CLIENT_ALIAS: &str = "hearthspace-fixture-snap-client-4a17f9";

#[test]
fn configured_theme_and_exported_snap_artwork_reach_desktop_icons() {
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
    write_png(
        &theme.join("48x48/apps/system-software-update.png"),
        UPDATER,
    );
    let steam_icon = fixture
        .path()
        .join("snap/steam/current/usr/share/icons/hicolor/256x256/apps/steam.png");
    write_png(&steam_icon, STEAM);
    fs::write(applications.join("update-manager.desktop"), "[Desktop Entry]\nType=Application\nName=Software Updater\nExec=/usr/bin/update-manager\nIcon=system-software-update\n").unwrap();
    // Use a unique alias: freedesktop-icons also searches /usr/share/pixmaps,
    // even with isolated XDG paths, so a host steam.png could mask this fallback.
    // Keep the provider outside applications until the child installs it below.
    fs::write(share.join("snap-client.desktop"), format!("[Desktop Entry]\nType=Application\nName=Steam\nExec=/snap/bin/steam %U\nIcon={}\nX-SnapAppName={CLIENT_ALIAS}\n", steam_icon.display())).unwrap();
    fs::write(applications.join("Z.A.T.O.  I Love the World and Everything In It.desktop"), format!("[Desktop Entry]\nType=Application\nName=Z.A.T.O. // I Love the World and Everything In It\nExec=steam steam://rungameid/4122860\nIcon={CLIENT_ALIAS}\n")).unwrap();

    // Isolate GSettings and the icon library's process-wide XDG caches from
    // other tests and the machine's installed themes and application catalog.
    let output = Command::new(env::current_exe().unwrap())
        .args(["--exact", "resolve_installed_icons_in_child", "--nocapture"])
        .env_clear()
        .env(CHILD_MODE, "1")
        .env("HOME", fixture.path().join("home"))
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
    let updater_artwork = app_icon::resolve(&TrayIcon {
        name: updater.icon.clone(),
        ..Default::default()
    })
    .expect("the configured icon theme supplies Software Updater's artwork");
    assert_eq!(&updater_artwork.rgba[..4], &UPDATER);

    let game = catalog
        .app_by_id("Z.A.T.O.  I Love the World and Everything In It.desktop")
        .unwrap();
    let icon = TrayIcon {
        name: game.icon.clone(),
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
    let game_artwork = app_icon::resolve(&icon)
        .expect("the game's declared Steam icon resolves through its exported Snap desktop entry");
    assert_eq!(&game_artwork.rgba[..4], &STEAM);
}

fn write_png(path: &Path, color: [u8; 4]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(48, 48, image::Rgba(color))
        .save(path)
        .unwrap();
}
