//! Match the native label's shaped width instead of estimating character cells.
use std::{cell::RefCell, collections::BTreeMap};
use xilem::masonry::{
    core::{BrushIndex, StyleProperty},
    parley::{FontContext, FontFamily, FontWeight, Layout, LayoutContext},
};

#[derive(Default)]
struct Metrics {
    fonts: FontContext,
    layouts: LayoutContext<BrushIndex>,
    layout: Layout<BrushIndex>,
    widths: BTreeMap<(String, u32), f64>,
}
thread_local! { static METRICS: RefCell<Metrics> = RefCell::new(Metrics::default()); }

pub(super) fn width(text: &str, size: f32) -> f64 {
    METRICS.with_borrow_mut(|metrics| {
        let key = (text.to_owned(), size.to_bits());
        if let Some(width) = metrics.widths.get(&key) {
            return *width;
        }
        let Metrics {
            fonts,
            layouts,
            layout,
            widths,
        } = metrics;
        let mut builder = layouts.ranged_builder(fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
            "Liberation Sans".into(),
        )));
        builder.push_default(StyleProperty::FontSize(size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(500.0)));
        builder.build_into(layout, text);
        layout.break_all_lines(None);
        let width = f64::from(layout.width()).ceil();
        // Clock and live status strings change; retain only a small warm cache.
        if widths.len() >= 64 {
            widths.clear();
        }
        widths.insert(key, width);
        width
    })
}
