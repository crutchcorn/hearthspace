use std::{env, fs, path::Path, process::Command};

use super::*;
use crate::{
    app_catalog::AppCatalog,
    ui::{dock, launcher::Entry, search_result, test_support::Ui, window_catalog::RunningWindow},
};
use xilem::{
    WidgetView,
    view::{flex_col, sized_box},
};

const CHILD_MODE: &str = "HEARTHSPACE_TEST_SHARED_APP_ICONS";
const HASH: &str = "279871be81ffe844e0ac24a1afea74af33126822";

#[test]
fn games_and_client_share_the_worker_without_sharing_their_artwork() {
    let fixture = tempfile::tempdir().unwrap();
    let home = fixture.path().join("home");
    let share = fixture.path().join("share");
    let applications = share.join("applications");
    fs::create_dir_all(&applications).unwrap();
    for (id, exec, class) in [
        ("first", "steam steam://rungameid/4122860", "FirstGame"),
        (
            "second",
            "steam steam://rungameid/9876543210987654321",
            "SecondGame",
        ),
        ("steam", "/snap/bin/steam %U", "Steam"),
    ] {
        fs::write(applications.join(format!("{id}.desktop")), format!(
            "[Desktop Entry]\nType=Application\nName={id}\nExec={exec}\nIcon=steam\nStartupWMClass={class}\n"
        )).unwrap();
    }
    for (root, id, color) in [
        (
            "snap/steam/common/.local/share/Steam",
            "4122860",
            [230, 20, 90],
        ),
        (".local/share/Steam", "9876543210987654321", [20, 230, 90]),
    ] {
        let path = home.join(format!("{root}/appcache/librarycache/{id}/{HASH}.jpg"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbImage::from_pixel(32, 32, image::Rgb(color))
            .save(path)
            .unwrap();
    }
    let theme = share.join("icons/hicolor");
    fs::create_dir_all(theme.join("48x48/apps")).unwrap();
    fs::write(theme.join("index.theme"), "[Icon Theme]\nName=hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\nContext=Applications\n").unwrap();
    write_png(&theme.join("48x48/apps/steam.png"));
    write_png(&theme.join("48x48/apps/org.example.Weather.png"));
    // A suffix may be part of the actual unmatched icon name as well.
    write_png(&theme.join("48x48/apps/org.example.Other.desktop.png"));
    let output = Command::new(env::current_exe().unwrap())
        .args([
            "--exact",
            "ui::dock::icons::tests::shared_application_artwork_in_child",
            "--nocapture",
        ])
        .env_clear()
        .env(CHILD_MODE, "1")
        .env("HOME", home)
        .env("XDG_DATA_HOME", &share)
        .env("XDG_DATA_DIRS", &share)
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
fn shared_application_artwork_in_child() {
    if env::var_os(CHILD_MODE).is_none() {
        return;
    }
    let mut state = ShellState::new("/unused".into(), vec!["test".into()]);
    state.catalog = AppCatalog::load();
    state.favorites.store.add("first.desktop").unwrap();
    state.favorites.store.add("second.desktop").unwrap();
    state.favorites.store.add("steam.desktop").unwrap();
    state.windows = vec![
        window(1, "FirstGame"),
        window(2, "SecondGame"),
        window(3, "org.example.Weather"),
        window(4, "org.example.Other.desktop"),
    ];
    let pending = requests(&state);
    assert_eq!(pending.len(), 5, "two games, client, and unmatched windows");
    let cache = decode_requests(Icons::new(), pending);
    state.dock.images = cache
        .iter()
        .map(|(key, pixmap)| (key.clone(), pixmap.clone().map(image)))
        .collect();
    let first = state.catalog.app_by_id("first.desktop").unwrap().clone();
    let second = state.catalog.app_by_id("second.desktop").unwrap();
    let client = state.catalog.app_by_id("steam.desktop").unwrap();
    let first_image = dock::app_image(&state, &first).unwrap();
    let second_image = dock::app_image(&state, second).unwrap();
    let client_image = dock::app_image(&state, client).unwrap();
    assert_ne!(
        first_image.image.data.data(),
        second_image.image.data.data()
    );
    assert_ne!(
        first_image.image.data.data(),
        client_image.image.data.data()
    );
    assert_ne!(
        second_image.image.data.data(),
        client_image.image.data.data()
    );
    assert_eq!(
        dock::window_image(&state, "FirstGame"),
        Some(first_image.clone())
    );
    assert_eq!(
        dock::window_image(&state, "SecondGame"),
        Some(second_image.clone())
    );
    assert!(dock::window_image(&state, "org.example.Weather").is_some());
    assert!(dock::window_image(&state, "org.example.weather").is_none());
    assert!(dock::window_image(&state, "org.example.Other.desktop").is_some());
    assert!(dock::window_image(&state, "org.example.Other").is_none());
    assert!(
        dock::image(&state, Some("steam")).is_none(),
        "app entries do not collide with named requests"
    );
    for group in apps(&state) {
        let key = group.icon.unwrap().key();
        assert!(
            cache[&key].is_some(),
            "every dock group reaches its decoded artwork"
        );
    }
    let expected = crate::app_icon::resolve_window(&state.catalog, "FirstGame").unwrap();
    assert_eq!(first_image.image.data.data(), expected.rgba.as_slice());
    let colors = [pixel(&first_image), pixel(&second_image)];

    // Check each consumer separately so a correct dock cannot mask a launcher
    // still reading the shared Icon=steam name (or the other way around).
    let mut ui = Ui::new(
        state,
        |state| sized_box(dock::view(state, &state.outputs[0], 420.0)),
        (420, 48),
    );
    let rendered = ui.harness.render();
    for color in colors {
        assert!(rendered.pixels().filter(|pixel| pixel.0 == color).count() > 50);
    }
    let mut ui = Ui::new(ui.state, shared_view, (420, 160));
    let rendered = ui.harness.render();
    for color in colors {
        assert!(rendered.pixels().filter(|pixel| pixel.0 == color).count() > 50);
    }

    // A metadata update must not reuse the previous result for the same ID.
    let custom = env::current_dir().unwrap().join("custom.png");
    write_png(&custom);
    let mut changed = first.clone();
    changed.icon = Some(custom.to_string_lossy().into_owned());
    let updated = decode_requests(cache, vec![Request::app(&changed)]);
    assert!(!updated.contains_key(&CacheKey::app(&first)));
    assert_eq!(
        updated[&CacheKey::app(&changed)],
        crate::app_icon::resolve_app(&changed)
    );
    let mut changed_exec = first.clone();
    changed_exec.exec = "steam steam://rungameid/9876543210987654321".into();
    let updated = decode_requests(updated, vec![Request::app(&changed_exec)]);
    assert_eq!(
        updated[&CacheKey::app(&changed_exec)]
            .as_ref()
            .unwrap()
            .rgba
            .as_slice(),
        second_image.image.data.data()
    );
}

fn shared_view(state: &ShellState) -> impl WidgetView<ShellState, Widget: Sized> + use<> {
    sized_box(flex_col((
        search_result::row(
            state,
            &state.outputs[0],
            Entry::App(Box::new(
                state.catalog.app_by_id("first.desktop").unwrap().clone(),
            )),
            false,
            Default::default(),
        ),
        search_result::row(
            state,
            &state.outputs[0],
            Entry::Window(state.windows[1].clone()),
            false,
            Default::default(),
        ),
    )))
}

fn pixel(brush: &ImageBrush) -> [u8; 4] {
    brush.image.data.data()[..4].try_into().unwrap()
}

fn window(id: u64, app_id: &str) -> RunningWindow {
    RunningWindow {
        id,
        app_id: app_id.into(),
        title: "Not an app identity".into(),
        focused: false,
    }
}

fn write_png(path: &Path) {
    image::RgbaImage::from_pixel(48, 48, image::Rgba([20, 40, 200, 255]))
        .save(path)
        .unwrap();
}
