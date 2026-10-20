use super::*;
use hearthspace_common::AccentColor;

#[test]
fn default_palette_preserves_existing_orange_light_materials() {
    let p = Palette::default();
    for (actual, expected) in [
        (p.app_background, crate::APP_BACKGROUND),
        (p.surface, crate::SURFACE),
        (p.surface_raised, crate::SURFACE_RAISED),
        (p.chrome, crate::CHROME),
        (p.chrome_raised, crate::CHROME_RAISED),
        (p.paper, crate::PAPER),
        (p.accent, crate::ACCENT),
        (p.accent_strong, crate::ACCENT_STRONG),
        (p.accent_ink, crate::ACCENT_INK),
        (p.text, crate::TEXT),
        (p.text_muted, crate::TEXT_MUTED),
        (p.accent_border, crate::ACCENT_BORDER),
        (p.accent_soft, crate::ACCENT_SOFT),
        (p.accent_wash, crate::ACCENT_WASH),
        (p.raised_button_hover, crate::RAISED_BUTTON_HOVER),
        (p.accent_bright, crate::settings_material::ACCENT_BRIGHT),
        (
            p.accent_decorative,
            crate::settings_material::ACCENT_DECORATIVE,
        ),
    ] {
        assert_eq!(actual, expected);
    }
}

#[test]
fn all_modes_and_accents_keep_shell_ink_readable_over_extreme_backdrops() {
    for color_mode in ColorMode::ALL {
        for accent_color in AccentColor::ALL {
            let p = Palette::new(Appearance {
                color_mode,
                accent_color,
            });
            let backdrop = if p.dark { Color::WHITE } else { Color::BLACK };
            for glass in [p.chrome_raised, p.surface.with_alpha(0.82)] {
                let surface = over(glass, backdrop);
                for background in [surface, over(p.accent_soft, surface)] {
                    for foreground in [p.text, p.text_muted, p.accent_strong] {
                        let contrast = math::contrast(foreground, background);
                        assert!(
                            contrast >= 4.5,
                            "{color_mode:?}/{accent_color:?}: {foreground:?} over {background:?}: {contrast}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn photo_material_is_reversible_without_changing_the_selected_accent() {
    for color_mode in ColorMode::ALL {
        for accent_color in AccentColor::ALL {
            let p = Palette::new(Appearance {
                color_mode,
                accent_color,
            });
            let photo = p.with_photo(true);
            assert_eq!(photo.appearance, p.appearance);
            assert_eq!(photo.accent, p.accent);
            assert!(photo.chrome.components[3] > p.chrome.components[3]);
            assert_eq!(photo.with_photo(false), p);
        }
    }
}
