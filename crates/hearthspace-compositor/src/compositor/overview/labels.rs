//! Cached Masonry labels stay readable independently of thumbnail scale.
use masonry::{
    core::{PropertySet, Widget},
    layout::AsUnit,
    parley::StyleProperty,
    peniko::Color,
    properties::{ContentColor, LineBreaking, types::CrossAxisAlignment},
    testing::{TestHarness, TestHarnessParams},
    theme::default_property_set,
    widgets::{Flex, Label, SizedBox},
};
use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::{Logical, Size, Transform},
};

pub(super) struct LabelBuffer {
    pub(super) text: String,
    pub(super) size: Size<i32, Logical>,
    pub(super) font_size: u32,
    pub(super) buffer: MemoryRenderBuffer,
}

impl LabelBuffer {
    pub(super) fn matches(&self, text: &str, size: Size<i32, Logical>, font_size: u32) -> bool {
        self.text == text && self.size == size && self.font_size == font_size
    }

    pub(super) fn new(text: String, size: Size<i32, Logical>, font_size: u32) -> Self {
        let image = rasterize(&text, size, font_size);
        Self {
            text,
            size,
            font_size,
            buffer: MemoryRenderBuffer::from_slice(
                &image.into_raw(),
                Fourcc::Abgr8888,
                (size.w.max(1), size.h.max(1)),
                1,
                Transform::Normal,
                None,
            ),
        }
    }
}

fn rasterize(text: &str, size: Size<i32, Logical>, font_size: u32) -> image::RgbaImage {
    let label = Label::new(text.to_string())
        .with_style(StyleProperty::FontSize(font_size as f32))
        .prepare()
        .with_props(
            PropertySet::new()
                .with(ContentColor::new(Color::from_rgb8(235, 240, 248)))
                .with(LineBreaking::Clip),
        );
    let row = Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .with(label, 1.0)
        .prepare();
    let widget = SizedBox::new(row)
        .width(size.w.max(1).px())
        .height(size.h.max(1).px())
        .prepare();
    let params = TestHarnessParams::default()
        .with_size((size.w.max(1) as u32, size.h.max(1) as u32))
        .with_background(Color::TRANSPARENT);
    TestHarness::create_with(default_property_set(), widget, params).render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_label_has_text_and_transparent_background() {
        let image = rasterize("Terminal — build logs", (260, 32).into(), 14);
        assert_eq!(image.dimensions(), (260, 32));
        assert!(image.pixels().filter(|pixel| pixel.0[3] > 100).count() > 100);
        assert_eq!(image.get_pixel(259, 31).0[3], 0);
    }
}
