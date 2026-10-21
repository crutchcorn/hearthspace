//! Shared shaping for off-thread sizing and the passive native caption.
use xilem::masonry::{
    core::{BrushIndex, StyleProperty},
    parley::{FontContext, FontFamily, FontWeight, Layout, LayoutContext, style::LineHeight},
};

#[derive(Default)]
pub(super) struct Metrics {
    fonts: FontContext,
    layouts: LayoutContext<BrushIndex>,
    layout: Layout<BrushIndex>,
}

impl Metrics {
    pub fn prepare(&mut self, title: &str, max_width: f64) -> (String, f64) {
        // Browser nowrap collapses line breaks; keep the complete title in the
        // accessible status while displaying one shaped, ellipsized line.
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let text_width = (max_width - 26.0).max(0.0);
        self.shape(&title);
        let width = (f64::from(self.layout.width()) + 26.0)
            .ceil()
            .min(max_width);
        if f64::from(self.layout.width()) <= text_width {
            return (title, width);
        }
        // Binary search complete character boundaries, keeping long Unicode
        // titles bounded without reshaping every removed character.
        let ends: Vec<_> = title
            .char_indices()
            .map(|(i, _)| i)
            .chain([title.len()])
            .collect();
        let (mut low, mut high) = (0, ends.len());
        while low < high {
            let middle = (low + high) / 2;
            self.shape(&format!("{}…", title[..ends[middle]].trim_end()));
            if f64::from(self.layout.width()) <= text_width {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let displayed = if low == 0 {
            String::new()
        } else {
            format!("{}…", title[..ends[low - 1]].trim_end())
        };
        (displayed, width)
    }

    fn shape(&mut self, text: &str) {
        shape(&mut self.fonts, &mut self.layouts, &mut self.layout, text);
    }
}

pub(super) fn shape(
    fonts: &mut FontContext,
    layouts: &mut LayoutContext<BrushIndex>,
    layout: &mut Layout<BrushIndex>,
    text: &str,
) {
    let mut builder = layouts.ranged_builder(fonts, text, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
        "'Helvetica Neue', 'Segoe UI', 'Liberation Sans', sans-serif".into(),
    )));
    builder.push_default(StyleProperty::FontSize(12.0));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(500.0)));
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.0)));
    builder.build_into(layout, text);
    layout.break_all_lines(None);
}
