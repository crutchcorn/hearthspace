//! Explicit live appearance tokens. A Palette is an ordinary Masonry property,
//! never a mutable process-wide theme; windows can retain independent palettes.
use hearthspace_common::{Appearance, ColorMode};
use std::{any::TypeId, sync::OnceLock};
use xilem::{
    Color,
    masonry::core::{Property, UpdateCtx},
};

pub(crate) mod classes;
mod math;
mod shell;
use math::{alpha, mix, over};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub appearance: Appearance,
    pub dark: bool,
    pub photo: bool,
    pub app_background: Color,
    pub field_dot: Color,
    pub surface: Color,
    pub surface_raised: Color,
    pub chrome: Color,
    pub chrome_raised: Color,
    pub chrome_raised_hover: Color,
    pub chrome_border: Color,
    pub paper: Color,
    pub surface_subtle: Color,
    pub surface_control: Color,
    pub border: Color,
    /// Exact reference ink for Settings and window decorations.
    pub text: Color,
    /// Contrast-safe shell foreground, including nested controls.
    pub text_on_glass: Color,
    /// Contrast-safe shell foreground.
    pub text_muted: Color,
    /// Exact reference ink for Settings and window decorations.
    pub muted: Color,
    pub accent_base: Color,
    pub accent: Color,
    pub accent_hover: Color,
    /// Contrast-safe shell foreground.
    pub accent_strong: Color,
    /// Reference --accent-strong for Settings and decorative details.
    pub accent_decorative: Color,
    pub accent_ink: Color,
    pub accent_border: Color,
    pub accent_soft: Color,
    pub accent_wash: Color,
    pub accent_bright: Color,
    pub accent_light: Color,
    pub focus_ring: Color,
    pub focus_ring_on_glass: Color,
    pub focus_ring_on_accent: Color,
    pub button_hover: Color,
    /// Shell-only hover tint; nested dark controls need a weaker white overlay.
    pub button_hover_on_glass: Color,
    pub raised_button_hover: Color,
    pub button_active: Color,
    pub control_track: Color,
    pub control_thumb: Color,
    pub scrollbar: Color,
    pub danger_subtle: Color,
    pub edge_light: Color,
    pub glass_edge_top: Color,
    pub glass_edge_bottom: Color,
    pub glass_edge_active_top: Color,
    pub glass_edge_active_border: Color,
    pub window_shadow: Color,
    pub panel_shadow: Color,
    pub glow_shadow: Color,
    pub window_accent_shadow: Color,
    pub panel_accent_shadow: Color,
    pub control_shadow: Color,
    pub control_inset_glow: Color,
    pub control_top_light: Color,
    /// Opaque approximation under client-side raised surfaces (not a blur).
    pub popup_backing: Color,
}

impl Palette {
    pub fn new(appearance: Appearance) -> Self {
        let dark = appearance.color_mode == ColorMode::Dark;
        let [r, g, b] = appearance.accent_color.rgb();
        let base = Color::from_rgb8(r, g, b);
        let accent = if dark {
            mix(base, 0.65, rgb(0xf9e5cc))
        } else {
            base
        };
        let surface = mix(
            base,
            if dark { 0.04 } else { 0.03 },
            if dark { rgb(0x202c30) } else { rgb(0xf6f8f5) },
        );
        let surface_raised = mix(surface, if dark { 0.88 } else { 0.55 }, Color::WHITE);
        let chrome = alpha(surface, if dark { 0.70 } else { 0.54 });
        let chrome_raised = alpha(surface_raised, 0.82);
        let accent_soft = if dark {
            mix(base, 0.14, rgba(0xffffff08))
        } else {
            mix(accent, 0.10, rgba(0xffffff24))
        };
        let accent_decorative = if dark {
            accent
        } else {
            mix(accent, 0.80, rgb(0x3d3028))
        };
        let muted = if dark { rgb(0xafbeb9) } else { rgb(0x667373) };
        let default_light = appearance == Appearance::default();
        let accent_strong = if default_light {
            crate::ACCENT_STRONG
        } else {
            accent_decorative
        };
        let text_muted = if default_light {
            crate::TEXT_MUTED
        } else {
            muted
        };
        let button_hover = if dark {
            rgba(0xe0f5ee12)
        } else {
            crate::BUTTON_HOVER
        };
        let app_background = if dark {
            rgb(0x142225)
        } else {
            crate::APP_BACKGROUND
        };
        let mut palette = Self {
            appearance,
            dark,
            photo: false,
            app_background,
            field_dot: if dark {
                rgba(0xd5f1e51c)
            } else {
                rgba(0x586f6826)
            },
            surface,
            surface_raised,
            chrome,
            chrome_raised,
            chrome_raised_hover: if dark {
                chrome_raised
            } else {
                mix(chrome_raised, 0.99, accent)
            },
            chrome_border: if dark {
                rgba(0xd9f3e82e)
            } else {
                crate::CHROME_BORDER
            },
            paper: alpha(surface, if dark { 0.25 } else { 0.30 }),
            surface_subtle: if dark {
                rgba(0xd9f3e805)
            } else {
                crate::SURFACE_SUBTLE
            },
            surface_control: if dark {
                rgba(0xd9f3e80a)
            } else {
                crate::SURFACE_CONTROL
            },
            border: if dark {
                rgba(0xd9f3e81a)
            } else {
                crate::BORDER
            },
            text: if dark { rgb(0xeff4ee) } else { crate::TEXT },
            text_on_glass: if dark { rgb(0xeff4ee) } else { crate::TEXT },
            text_muted,
            muted,
            accent_base: base,
            accent,
            accent_hover: mix(accent, 0.82, Color::WHITE),
            accent_strong,
            accent_decorative,
            accent_ink: if dark {
                rgb(0x202827)
            } else {
                crate::ACCENT_INK
            },
            accent_border: if dark {
                mix(accent, 0.42, rgba(0xffffff20))
            } else {
                mix(accent, 0.36, rgba(0xffffff80))
            },
            accent_soft,
            accent_wash: if dark {
                mix(base, 0.10, rgba(0xffffff03))
            } else {
                mix(accent, 0.07, rgba(0xffffff14))
            },
            accent_bright: mix(accent, 0.64, Color::WHITE),
            accent_light: mix(accent, 0.38, rgb(0xfff8f0)),
            focus_ring: accent,
            focus_ring_on_glass: accent_strong,
            focus_ring_on_accent: if default_light {
                crate::FOCUS_RING_ON_ACCENT
            } else if dark {
                rgb(0x202827)
            } else {
                mix(accent, 0.30, Color::WHITE)
            },
            button_hover,
            button_hover_on_glass: if dark {
                // 18/255 over the launcher's header and tab container exceeds
                // the luminance where even white text can reach 4.5:1.
                rgba(0xe0f5ee0c)
            } else {
                button_hover
            },
            raised_button_hover: if dark {
                button_hover
            } else {
                mix(accent, 0.05, rgba(0xffffff33))
            },
            button_active: accent_soft,
            control_track: if dark {
                rgba(0xd9f3e82b)
            } else {
                crate::CONTROL_TRACK
            },
            control_thumb: if dark {
                rgb(0xeff4ee)
            } else {
                crate::CONTROL_THUMB
            },
            scrollbar: if dark {
                rgba(0xc2dcd238)
            } else {
                rgba(0x52696138)
            },
            danger_subtle: if dark {
                mix(rgb(0xb84726), 0.18, surface)
            } else {
                crate::DANGER_SUBTLE
            },
            edge_light: accent.with_alpha(if dark { 0.06 } else { 0.05 }),
            glass_edge_top: if dark {
                rgba(0xe5fff214)
            } else {
                rgba(0xffffff85)
            },
            glass_edge_bottom: if dark {
                rgba(0x0816161a)
            } else {
                rgba(0x49635b0a)
            },
            glass_edge_active_top: if dark {
                rgba(0xe5fff22e)
            } else {
                rgba(0xffffffc7)
            },
            glass_edge_active_border: if dark {
                rgba(0xe5fff20a)
            } else {
                rgba(0xffffff30)
            },
            window_shadow: if dark {
                rgba(0x06141670)
            } else {
                rgba(0x354e3b24)
            },
            panel_shadow: if dark {
                rgba(0x06141680)
            } else {
                rgba(0x354e3b2b)
            },
            glow_shadow: if dark {
                rgba(0x06141680)
            } else {
                rgba(0x354e3b30)
            },
            window_accent_shadow: accent.with_alpha(if dark { 0.10 } else { 0.15 }),
            panel_accent_shadow: accent.with_alpha(if dark { 0.12 } else { 0.16 }),
            control_shadow: accent.with_alpha(if dark { 0.12 } else { 0.14 }),
            control_inset_glow: accent.with_alpha(if dark { 0.06 } else { 0.04 }),
            control_top_light: if dark {
                Color::TRANSPARENT
            } else {
                rgba(0xffffffa6)
            },
            popup_backing: if default_light {
                rgb(0xeff0eb)
            } else {
                over(chrome, app_background)
            },
        };
        shell::resolve_ink(&mut palette);
        palette
    }

    /// Adapt the reference palette for translucent shell controls. Keep this
    /// copy local to the shell; Settings and compositor chrome use the original.
    pub fn for_glass(mut self) -> Self {
        self.text = self.text_on_glass;
        self.button_hover = self.button_hover_on_glass;
        if self.dark {
            self.raised_button_hover = self.button_hover_on_glass;
        }
        self
    }

    pub fn with_photo(mut self, photo: bool) -> Self {
        self.photo = photo;
        self.chrome = alpha(
            self.surface,
            match (photo, self.dark) {
                (true, true) => 0.80,
                (true, false) => 0.76,
                (false, true) => 0.70,
                (false, false) => 0.54,
            },
        );
        self.muted = if photo {
            mix(self.text, 0.80, self.surface)
        } else if self.dark {
            rgb(0xafbeb9)
        } else {
            rgb(0x667373)
        };
        self
    }

    pub fn prop_changed(ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<Self>() {
            ctx.request_paint_only();
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new(Appearance::default())
    }
}
impl Property for Palette {
    fn static_default() -> &'static Self {
        static DEFAULT: OnceLock<Palette> = OnceLock::new();
        DEFAULT.get_or_init(Self::default)
    }
}

fn rgb(value: u32) -> Color {
    Color::from_rgb8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}
fn rgba(value: u32) -> Color {
    Color::from_rgba8(
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}
