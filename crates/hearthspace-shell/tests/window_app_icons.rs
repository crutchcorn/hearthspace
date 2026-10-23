use std::{fs, path::Path};

use hearthspace_shell::{
    app_catalog::AppCatalog,
    app_icon,
    tray::{TrayIcon, TrayPixmap},
};

fn write_app(root: &Path, id: &str, fields: &str, color: [u8; 4]) {
    let applications = root.join("applications");
    fs::create_dir_all(&applications).unwrap();
    let icon = root.join(format!("{id}.png"));
    image::RgbaImage::from_pixel(48, 48, image::Rgba(color))
        .save(&icon)
        .unwrap();
    fs::write(
        applications.join(id),
        format!(
            "[Desktop Entry]\nType=Application\nName={id}\nIcon={}\n{fields}\n",
            icon.display()
        ),
    )
    .unwrap();
}

fn desktop_icon(catalog: &AppCatalog, id: &str) -> TrayPixmap {
    app_icon::resolve(&TrayIcon {
        name: catalog.app_by_id(id).unwrap().icon.clone(),
        ..Default::default()
    })
    .expect("desktop artwork decodes")
}

#[test]
fn snap_client_windows_use_the_clients_desktop_artwork_with_games_installed() {
    let root = tempfile::tempdir().unwrap();
    write_app(
        root.path(),
        "steam_steam.desktop",
        "Exec=/snap/bin/steam %U\nX-SnapAppName=steam",
        [20, 40, 70, 255],
    );
    write_app(
        root.path(),
        "Pepper Grinder.desktop",
        "Exec=steam steam://rungameid/2076580",
        [250, 160, 30, 255],
    );
    let catalog = AppCatalog::load_from_data_dirs(vec![root.path().to_owned()]);
    let client = desktop_icon(&catalog, "steam_steam.desktop");
    let game = desktop_icon(&catalog, "Pepper Grinder.desktop");
    assert_ne!(client, game);
    assert_eq!(app_icon::resolve_window(&catalog, "Steam"), Some(client));
    assert_eq!(
        app_icon::resolve_window(&catalog, "PepperGrinder"),
        Some(game)
    );
    assert!(
        catalog
            .app_for_window("Update Required - Pepper Grinder")
            .is_none()
    );
}

#[test]
fn literal_desktop_suffix_and_unrelated_window_class_reach_declared_artwork() {
    let root = tempfile::tempdir().unwrap();
    write_app(
        root.path(),
        "ai.opencode.desktop.desktop",
        "Exec=/opt/OpenCode/ai.opencode.desktop %U\nStartupWMClass=ai.opencode.desktop",
        [0, 200, 80, 255],
    );
    write_app(
        root.path(),
        "com.example.Editor.desktop",
        "Exec=/opt/editor/start\nStartupWMClass=EditorWindow",
        [120, 30, 200, 255],
    );
    let catalog = AppCatalog::load_from_data_dirs(vec![root.path().to_owned()]);
    let editor = desktop_icon(&catalog, "com.example.Editor.desktop");
    let opencode = desktop_icon(&catalog, "ai.opencode.desktop.desktop");
    assert_eq!(
        app_icon::resolve_window(&catalog, "ai.opencode.desktop"),
        Some(opencode.clone())
    );
    assert_eq!(
        app_icon::resolve_window(&catalog, "ai.opencode.desktop.desktop"),
        Some(opencode)
    );
    assert_eq!(
        app_icon::resolve_window(&catalog, "EditorWindow"),
        Some(editor)
    );
}
