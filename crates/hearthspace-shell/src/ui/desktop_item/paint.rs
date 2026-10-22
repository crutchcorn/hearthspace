use super::{Artwork, widget::DesktopTile};
use hearthspace_ui_components::{Palette, settings_material::shadow};
use xilem::{
    Color,
    masonry::{
        core::render_text,
        imaging::Painter,
        kurbo::{Affine, BezPath, Rect, Shape, Stroke, Vec2},
        peniko::Gradient,
    },
};

const BOUNDS: Rect = Rect::new(0.0, 0.0, super::ITEM_WIDTH, super::ITEM_HEIGHT);

pub(super) fn tile(
    painter: &mut Painter<'_>,
    tile: &DesktopTile,
    palette: &Palette,
    hovered: bool,
) {
    let photo = tile.state.photo_background;
    let pale = pale_ink(palette);
    let selected = tile.state.selected;
    let shape = BOUNDS.inset(-0.5).to_rounded_rect(10.0);
    if hovered || selected {
        shadow(
            painter,
            BOUNDS,
            10.0,
            palette.control_shadow,
            (0.0, 6.0),
            if palette.dark && !(photo && selected) {
                24.0
            } else {
                22.0
            },
            -5.0,
        );
        painter
            .fill(
                shape,
                if selected {
                    palette.accent_soft
                } else {
                    palette.surface_control
                },
            )
            .draw();
        if !(photo && selected) {
            let glow = Gradient::new_radial(BOUNDS.center(), 48.0)
                .with_stops([(0.0, Color::TRANSPARENT), (1.0, palette.control_inset_glow)]);
            painter.fill(shape, &glow).draw();
        }
        let (border, top, rim) = if photo && selected {
            (
                pale.with_alpha(0.72),
                pale.with_alpha(0.55),
                pale.with_alpha(0.24),
            )
        } else {
            (
                if selected {
                    palette.accent_border
                } else {
                    palette.chrome_border
                },
                palette.glass_edge_active_top,
                palette.glass_edge_active_border,
            )
        };
        // Insets sit inside the CSS border; the top edge follows its corner.
        let inner = BOUNDS.inset(-1.0).to_rounded_rect(9.0);
        painter.with_fill_clip(inner, |painter| {
            let shifted = inner + Vec2::new(0.0, 1.0);
            let edge = BezPath::from_iter(
                BOUNDS
                    .path_elements(0.1)
                    .chain(shifted.to_path(0.1).reverse_subpaths()),
            );
            painter.fill(edge, top).draw();
        });
        painter.stroke(inner, &Stroke::new(1.0), rim).draw();
        painter.stroke(shape, &Stroke::new(1.0), border).draw();
    }
    if tile.state.focused && tile.state.keyboard_focus {
        painter
            .stroke(
                BOUNDS.inflate(3.0, 3.0).to_rounded_rect(13.0),
                &Stroke::new(2.0),
                if photo {
                    pale
                } else {
                    palette.accent_decorative
                },
            )
            .draw();
    }

    let height = tile.caption.height();
    // The website flexes its label box down to 22px when a second line wraps,
    // while leaving the two-line text visible beyond that box.
    let icon_y = (96.0 - 56.0 - 4.0 - height.min(22.0)) / 2.0;
    let icon_box = Rect::new(16.0, icon_y, 72.0, icon_y + 56.0);
    artwork(painter, tile, icon_box, palette);
    let origin = (7.0, icon_y + 60.0);
    let text_color = if photo { Color::WHITE } else { palette.text };
    let draw_text = |painter: &mut Painter<'_>, offset: Vec2, ink: Color| {
        render_text(
            painter,
            Affine::translate((origin.0 + offset.x, origin.1 + offset.y)),
            &tile.caption.layout,
            &[ink.into()],
            true,
        );
    };
    if photo {
        gaussian_shadow(
            painter,
            (0.0, 0.0),
            1.5,
            Color::BLACK.with_alpha(0.6),
            draw_text,
        );
        gaussian_shadow(
            painter,
            (0.0, 1.0),
            1.0,
            Color::BLACK.with_alpha(0.867),
            draw_text,
        );
    } else {
        gaussian_shadow(painter, (0.0, 1.0), 1.0, palette.app_background, draw_text);
    }
    draw_text(painter, Vec2::ZERO, text_color);
}

fn artwork(painter: &mut Painter<'_>, tile: &DesktopTile, bounds: Rect, palette: &Palette) {
    let photo = tile.state.photo_background;
    if let Artwork::App(Some(image)) | Artwork::File(Some(image)) = &tile.content.artwork {
        let scale = 49.4 / f64::from(image.image.width.max(image.image.height).max(1));
        painter.draw_image(
            image,
            Affine::translate((
                bounds.center().x - f64::from(image.image.width) * scale / 2.0,
                bounds.center().y - f64::from(image.image.height) * scale / 2.0,
            )) * Affine::scale(scale),
        );
        return;
    }
    let size = match tile.content.artwork {
        Artwork::Folder => 46.0,
        Artwork::File(_) => 42.0,
        _ => 38.0,
    };
    let rect = Rect::from_center_size(bounds.center(), (size, size));
    let ink = if photo {
        pale_ink(palette)
    } else if matches!(tile.content.artwork, Artwork::File(_)) {
        palette.muted
    } else {
        palette.accent_decorative
    };
    let backing = if photo {
        None
    } else {
        match tile.content.artwork {
            Artwork::Folder => Some(palette.accent_soft),
            Artwork::File(_) => Some(palette.chrome_raised),
            _ => None,
        }
    };
    if photo {
        let draw = |painter: &mut Painter<'_>, delta: Vec2, color| {
            tile.glyph.paint(painter, rect + delta, color, None)
        };
        gaussian_shadow(painter, (0.0, 2.0), 1.5, Color::BLACK.with_alpha(0.5), draw);
        gaussian_shadow(
            painter,
            (0.0, 0.0),
            0.5,
            Color::from_rgb8(0x14, 0x22, 0x25),
            draw,
        );
    }
    tile.glyph.paint(painter, rect, ink, backing);
}

pub(super) fn pale_ink(palette: &Palette) -> Color {
    let [r, g, b, _] = palette.accent_base.components;
    Color::new([r * 0.18 + 0.82, g * 0.18 + 0.82, b * 0.18 + 0.82, 1.0])
}

/// Masonry has no glyph/filter layer. Sample a normalized Gaussian over the
/// shaped glyphs, preserving their silhouettes and the unclipped outer halo.
fn gaussian_shadow(
    painter: &mut Painter<'_>,
    origin: (f64, f64),
    sigma: f64,
    color: Color,
    draw: impl Fn(&mut Painter<'_>, Vec2, Color),
) {
    const KERNEL: [f32; 5] = [
        0.054_488_685,
        0.244_201_35,
        0.402_619_96,
        0.244_201_35,
        0.054_488_685,
    ];
    for (y, wy) in KERNEL.iter().enumerate() {
        for (x, wx) in KERNEL.iter().enumerate() {
            draw(
                painter,
                Vec2::new(
                    origin.0 + (x as f64 - 2.0) * sigma,
                    origin.1 + (y as f64 - 2.0) * sigma,
                ),
                color.multiply_alpha(wx * wy),
            );
        }
    }
}
