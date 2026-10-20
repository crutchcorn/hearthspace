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
        (p.text_on_glass, crate::TEXT),
        (p.button_hover_on_glass, crate::BUTTON_HOVER),
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
            let shell = p.for_glass();
            let backdrop = if p.dark { Color::WHITE } else { Color::BLACK };
            for glass in [p.chrome_raised, math::alpha(p.surface, 0.82)] {
                let panel = over(glass, backdrop);
                let header = over(p.surface_control, panel);
                let tabs = over(p.surface_subtle, header);
                let row = over(p.surface_subtle, panel);
                // These mirror visible control hierarchies, rather than just
                // a single swatch: launcher header → tabs → selection/hover,
                // and a selected result row → its fallback app artwork.
                for (name, background) in [
                    ("panel", panel),
                    ("selected control", over(p.accent_soft, panel)),
                    ("header", header),
                    ("tabs", tabs),
                    ("selected tab", over(p.accent_soft, tabs)),
                    ("hovered tab", over(shell.button_hover, tabs)),
                    ("result row", row),
                    ("result artwork", over(p.accent_soft, row)),
                ] {
                    for foreground in [shell.text, shell.text_muted, shell.accent_strong] {
                        let ratio = math::contrast(foreground, background);
                        assert!(
                            ratio >= 4.5,
                            "{color_mode:?}/{accent_color:?}/{name}: {foreground:?} over {background:?}: {ratio}"
                        );
                    }
                }
            }
            assert_eq!(shell.chrome_raised, p.chrome_raised);
            assert_eq!(shell.accent, p.accent);
            assert_eq!(shell.accent_soft, p.accent_soft);
            assert_eq!(shell.for_glass(), shell);
            if p.dark {
                assert_eq!(p.text, rgb(0xeff4ee));
                assert_eq!(p.button_hover, rgba(0xe0f5ee12));
            } else {
                assert_eq!(shell, p, "light materials retain their original pixels");
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
