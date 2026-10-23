//! Shared shader uniforms for window frames and detached search panel shadows.
use super::{FrameLayer, FrameStyle};
use hearthspace_ui_components::Palette;
use smithay::{
    backend::renderer::{
        element::Kind,
        gles::{GlesPixelProgram, Uniform, element::PixelShaderElement},
    },
    utils::{Logical, Rectangle},
};

pub(super) fn make_element(
    shader: &GlesPixelProgram,
    style: FrameStyle,
    layer: FrameLayer,
    palette: &Palette,
) -> PixelShaderElement {
    let (area, uniforms) = appearance(style, layer, palette);
    PixelShaderElement::new(shader.clone(), area, None, 1.0, uniforms, Kind::Unspecified)
}

pub(super) fn appearance(
    style: FrameStyle,
    layer: FrameLayer,
    palette: &Palette,
) -> (Rectangle<i32, Logical>, Vec<Uniform<'static>>) {
    // Three sigma of the broadest reference shadow, including its displacement.
    // The transparent interior is discarded by the shader and opaque clients
    // let Smithay subtract their body from the damaged shadow regions.
    let padding = (match layer {
        FrameLayer::Rim => 1.0,
        FrameLayer::Shadow => 144.0,
        FrameLayer::Header => 0.0,
    } * style.zoom)
        .ceil() as i32;
    let mut area = Rectangle::new(
        (
            style.window.loc.x.saturating_sub(padding),
            style.window.loc.y.saturating_sub(padding),
        )
            .into(),
        (
            style
                .window
                .size
                .w
                .saturating_add(padding.saturating_mul(2)),
            style
                .window
                .size
                .h
                .saturating_add(padding.saturating_mul(2)),
        )
            .into(),
    );
    if matches!(layer, FrameLayer::Header) {
        area.size.h = ((f64::from(crate::config::TITLE_BAR_HEIGHT) * style.zoom).round() as i32)
            .min(style.window.size.h)
            .max(1);
    }
    let mut uniforms = vec![
        Uniform::new("scene_opacity", style.opacity),
        Uniform::new("area_size", (area.size.w as f32, area.size.h as f32)),
        Uniform::new(
            "window_size",
            (style.window.size.w as f32, style.window.size.h as f32),
        ),
        Uniform::new(
            "window_offset",
            (
                (style.window.loc.x - area.loc.x) as f32,
                (style.window.loc.y - area.loc.y) as f32,
            ),
        ),
        Uniform::new("zoom", style.zoom as f32),
        Uniform::new(
            "radius",
            if style.panel {
                style.panel_radius * style.zoom as f32
            } else if style.server_side {
                19.0 * style.zoom as f32
            } else {
                0.0
            },
        ),
        Uniform::new("active", if style.active { 1.0f32 } else { 0.0 }),
        Uniform::new("hovered", if style.hovered { 1.0f32 } else { 0.0 }),
        Uniform::new("panel", if style.panel { 1.0f32 } else { 0.0 }),
        Uniform::new(
            "mode",
            match layer {
                FrameLayer::Rim => 0.0f32,
                FrameLayer::Shadow => 1.0,
                FrameLayer::Header => 2.0,
            },
        ),
    ];
    let p = palette.with_photo(style.photo);
    let raised = style.active || style.hovered;
    let (broad, near) = if style.active {
        (
            p.accent.with_alpha(if p.dark { 0.14 } else { 0.18 }),
            p.glow_shadow,
        )
    } else if style.hovered || style.panel {
        (p.panel_shadow, p.panel_accent_shadow)
    } else {
        (p.window_shadow, p.window_accent_shadow)
    };
    for (name, color) in [
        ("chrome_color", p.chrome),
        ("wash_color", p.edge_light),
        ("control_color", p.surface_control),
        ("line_color", p.border),
        ("muted_color", p.muted),
        (
            "border_color",
            if style.active {
                p.accent_border
            } else {
                p.chrome_border
            },
        ),
        (
            "edge_color",
            if raised {
                p.glass_edge_active_top
            } else {
                p.glass_edge_top
            },
        ),
        (
            "edge_all_color",
            if raised {
                p.glass_edge_active_border
            } else {
                xilem::Color::TRANSPARENT
            },
        ),
        ("broad_shadow_color", broad),
        ("near_shadow_color", near),
        ("inset_color", p.edge_light),
    ] {
        uniforms.push(Uniform::new(name, color.components));
    }
    (area, uniforms)
}
