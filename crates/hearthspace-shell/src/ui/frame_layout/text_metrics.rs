//! Match the native label's shaped width instead of estimating character cells.
use std::{cell::RefCell, collections::BTreeMap};
use xilem::masonry::{
    core::{BrushIndex, StyleProperty},
    parley::{FontContext, FontFamily, FontWeight, Layout, LayoutContext},
};

struct Metrics {
    fonts: FontContext,
    layouts: LayoutContext<BrushIndex>,
    layout: Layout<BrushIndex>,
    widths: BTreeMap<(String, u32), f64>,
}
impl Default for Metrics {
    fn default() -> Self {
        Self {
            fonts: font_context(),
            layouts: LayoutContext::default(),
            layout: Layout::default(),
            widths: BTreeMap::new(),
        }
    }
}

#[cfg(not(test))]
fn font_context() -> FontContext {
    FontContext::default()
}

#[cfg(test)]
fn font_context() -> FontContext {
    use xilem::masonry::{
        parley::fontique::{Collection, CollectionOptions, FallbackKey, Script},
        peniko::Blob,
        testing::ROBOTO,
    };
    // Masonry's harness disables system fonts and installs this Latin fallback.
    // Measure with that same font environment when testing native view geometry.
    let mut fonts = FontContext {
        collection: Collection::new(CollectionOptions {
            system_fonts: false,
            ..Default::default()
        }),
        ..Default::default()
    };
    let families = fonts
        .collection
        .register_fonts(Blob::new(std::sync::Arc::new(ROBOTO)), None);
    fonts.collection.append_fallbacks(
        FallbackKey::new(Script::from_bytes(*b"Latn"), None),
        families.iter().map(|(family, _)| *family),
    );
    fonts
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
