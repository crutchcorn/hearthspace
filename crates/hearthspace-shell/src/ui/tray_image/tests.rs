use super::*;
use crate::ui::{state::ShellState, test_support::Ui, tray_button, tray_tests::item};
use hearthspace_common::{Appearance, ColorMode};
use hearthspace_ui_components::Palette;
use xilem::{WidgetView, view::sized_box};

fn view(
    s: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(tray_button::icon(
        "A",
        &s.tray.snapshot.items[0],
        s.appearance.palette,
    ))
}
fn ink(mode: ColorMode) -> Color {
    Palette::new(Appearance {
        color_mode: mode,
        ..Default::default()
    })
    .for_glass()
    .text
}
fn source() -> TrayPixmap {
    TrayPixmap {
        width: 2,
        height: 2,
        rgba: vec![
            255, 255, 255, 255, 255, 255, 255, 128, 0, 0, 0, 0, 255, 255, 255, 255,
        ],
    }
}
#[test]
fn white_symbolic_svg_tracks_live_theme_without_a_new_application_icon() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("telegram-symbolic.svg");
    std::fs::write(&path,r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path fill="#FFFFFF" d="M2 2h12v12H2z"/></svg>"##).unwrap();
    let name = path.to_string_lossy().into_owned();
    let pixmap =
        hearthspace_desktop::app_icon::resolve(&hearthspace_desktop::artwork::IconSource {
            name: Some(name.clone()),
            ..Default::default()
        })
        .unwrap();
    let mut state = ShellState::new("/unused".into(), vec!["A".into()]);
    let mut app = item(1);
    app.icon.name = Some(name);
    app.icon.pixmap = Some(pixmap);
    state.tray.snapshot.items = vec![app];
    let original = state.tray.snapshot.items[0].clone();
    let mut ui = Ui::new(state, view, (34, 34));
    for mode in [ColorMode::Light, ColorMode::Dark, ColorMode::Light] {
        ui.state.appearance.update(Palette::new(Appearance {
            color_mode: mode,
            ..Default::default()
        }));
        ui.rebuild(&view);
        ui.dispatch(&view);
        let image = ui.harness.render();
        if let Some(base) = std::env::var_os("HEARTHSPACE_SHELL_EVIDENCE") {
            let base = std::path::PathBuf::from(base).join("tray-contrast");
            std::fs::create_dir_all(&base).unwrap();
            image.save(base.join(format!("{mode:?}.png"))).unwrap();
        }
        let pixel = image.get_pixel(17, 17).0;
        assert!(
            if mode == ColorMode::Light {
                pixel[0] < 100
            } else {
                pixel[0] > 200
            },
            "{mode:?}: {pixel:?}"
        );
        assert_eq!(
            ui.state.tray.snapshot.items[0], original,
            "theme changes must not mutate the app's icon or input identity"
        );
    }
}
#[test]
fn unnamed_white_pixmap_changes_ink_without_changing_alpha_or_source() {
    let raw = source();
    let light = themed(&raw, None, ink(ColorMode::Light));
    let dark = themed(&raw, None, ink(ColorMode::Dark));
    assert!(light.rgba[0] < 100);
    assert!(dark.rgba[0] > 200);
    assert_eq!(
        light
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect::<Vec<_>>(),
        vec![255, 128, 0, 255]
    );
    assert_eq!(raw, source());
}
#[test]
fn symbolic_status_badges_keep_their_colors_and_opaque_tiles_are_not_flattened() {
    let mut raw = source();
    raw.rgba[..4].copy_from_slice(&[220, 20, 30, 255]);
    let recolored = themed(
        &raw,
        Some("snap.telegram-desktop.-attention-symbolic"),
        ink(ColorMode::Light),
    );
    assert_eq!(&recolored.rgba[..4], &[220, 20, 30, 255]);
    assert!(recolored.rgba[4] < 100);
    let tile = TrayPixmap {
        width: 2,
        height: 1,
        rgba: vec![240, 240, 240, 255, 20, 20, 20, 255],
    };
    assert_eq!(themed(&tile, None, ink(ColorMode::Light)), tile);
}
#[test]
fn colored_logo_and_attention_replacement_keep_original_artwork() {
    let mut state = ShellState::new("/unused".into(), vec!["A".into()]);
    let mut app = item(1);
    app.icon.pixmap = Some(source());
    state.tray.snapshot.items = vec![app];
    let mut ui = Ui::new(state, view, (34, 34));
    let replacement = TrayPixmap {
        width: 2,
        height: 2,
        rgba: vec![
            20, 60, 220, 255, 0, 0, 0, 0, 20, 60, 220, 255, 20, 60, 220, 255,
        ],
    };
    let app = &mut ui.state.tray.snapshot.items[0];
    app.icon.name = Some("steam".into());
    app.icon.pixmap = Some(replacement.clone());
    app.status = crate::tray::TrayStatus::NeedsAttention;
    ui.rebuild(&view);
    ui.dispatch(&view);
    assert_eq!(
        themed(&replacement, Some("steam"), ink(ColorMode::Light)),
        replacement
    );
    let image = ui.harness.render();
    let pixel = image.get_pixel(12, 17).0;
    assert!(
        pixel[2] > pixel[0] + 100,
        "colored replacement must remain blue: {pixel:?}"
    );
}

#[test]
fn explicitly_monochrome_steam_art_adapts_but_unmarked_multitone_art_does_not() {
    let raw = TrayPixmap {
        width: 3,
        height: 1,
        rgba: vec![50, 50, 50, 255, 210, 210, 210, 255, 0, 0, 0, 0],
    };
    assert_eq!(themed(&raw, Some("app-logo"), ink(ColorMode::Light)), raw);
    let adapted = themed(
        &raw,
        Some("/fixture/steam_tray_mono.png"),
        ink(ColorMode::Light),
    );
    assert_eq!(&adapted.rgba[..3], &adapted.rgba[4..7]);
    assert!(adapted.rgba[0] < 100);
}
