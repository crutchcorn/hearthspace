//! Two-line native text, with real shaping, emergency wrapping, and ellipsis.
use xilem::{
    TextAlign,
    masonry::{
        TextAlignOptions,
        core::{BrushIndex, StyleProperty},
        parley::{
            FontContext, FontFamily, FontWeight, Layout, LayoutContext,
            style::{LineHeight, OverflowWrap},
        },
    },
};

const WIDTH: f32 = (super::ITEM_WIDTH - 14.0) as f32;

#[derive(Default)]
pub(super) struct Caption {
    pub layout: Layout<BrushIndex>,
    valid: bool,
    pub displayed: String,
}

impl Caption {
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    pub fn build(
        &mut self,
        fonts: &mut FontContext,
        layouts: &mut LayoutContext<BrushIndex>,
        text: &str,
    ) {
        if self.valid {
            return;
        }
        let mut displayed = text.to_owned();
        self.shape(fonts, layouts, &displayed);
        if self.layout.len() > 2 {
            // Start at the second line's end, then remove complete Unicode
            // characters until the ellipsis also fits. Never slice UTF-8 bytes.
            let end = self.layout.get(1).unwrap().text_range().end;
            displayed.truncate(end);
            loop {
                let candidate = format!("{}…", displayed.trim_end());
                self.shape(fonts, layouts, &candidate);
                if self.layout.len() <= 2 {
                    displayed = candidate;
                    break;
                }
                if displayed.pop().is_none() {
                    break;
                }
            }
        }
        self.layout
            .align(Some(WIDTH), TextAlign::Center, TextAlignOptions::default());
        self.displayed = displayed;
        self.valid = true;
    }

    fn shape(
        &mut self,
        fonts: &mut FontContext,
        layouts: &mut LayoutContext<BrushIndex>,
        text: &str,
    ) {
        let mut builder = layouts.ranged_builder(fonts, text, 1.0, true);
        // Match Settings: the reference browser's Linux sans-serif resolves to
        // Liberation Sans, while Parley's generic fallback can have wider metrics.
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
            "'Helvetica Neue', 'Segoe UI', 'Liberation Sans', sans-serif".into(),
        )));
        builder.push_default(StyleProperty::FontSize(12.0));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(500.0)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.3)));
        builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        builder.build_into(&mut self.layout, text);
        self.layout.break_all_lines(Some(WIDTH));
    }

    pub fn height(&self) -> f64 {
        f64::from(self.layout.height())
    }
}
